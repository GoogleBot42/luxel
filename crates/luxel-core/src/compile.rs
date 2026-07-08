//! AST → bytecode compiler.
//!
//! Scoping rules (matching the documented PB semantics):
//! - `var` is function-scoped and hoisted (JS `var` rules); `var` at top
//!   level declares a global.
//! - Assignment to an undeclared name creates a global — even from inside a
//!   function or lambda. A collection pre-pass finds every such name so
//!   forward references across functions resolve.
//! - No closures: a lambda sees only its own params/locals and globals.
//! - `pixelCount` and the math constants are predefined globals.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ast::*;
use crate::bytecode::op;
use crate::diag::{line_col, Diagnostic, Span};
use crate::fixed::Fx;
use crate::parse::parse_program;
use crate::vm::{lookup_builtin, lookup_method, FnDef, GlobalDef, Program, Value};

/// Compiler IR: one virtual instruction, jump targets as INSTRUCTION
/// INDICES. This never reaches the VM — [`assemble`] lowers it to the flat
/// LXBC byte encoding (byte-offset jumps) that `Program.code` holds and the
/// interpreter executes in place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Insn {
    Const(Value),
    LoadG(u16),
    StoreG(u16), // pops value, pushes it back (assignment is an expression)
    LoadL(u8),
    StoreL(u8), // ditto
    LoadIdx,    // [arr idx] → [elem]
    StoreIdx,   // [arr idx val] → [val]
    ArrLen,     // [arr] → [len]
    NewArray(u16),
    Dup,
    Dup2, // [a b] → [a b a b]
    Pop,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Neg,
    Not,
    BitNot,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    Jmp(u32),
    JmpIfFalse(u32),     // pops
    JmpIfTruePeek(u32),  // ||: jump keeping the lhs value
    JmpIfFalsePeek(u32), // &&
    CallFn { fn_idx: u16, argc: u8 },
    CallBuiltin { b: u16, argc: u8 },
    CallValue { argc: u8 },
    Ret,
    RetNull,
}

const MAX_GLOBALS: usize = 256;
// 255, not 256: FnDef.locals is a u8 slot *count* (a 256th slot would wrap
// it to 0) and LoadL/StoreL operands are u8 slot indices.
const MAX_LOCALS: usize = 255;

pub fn compile(src: &str) -> Result<Program, Diagnostic> {
    let ast = parse_program(src)?;
    let mut c = Compiler::new(src);
    c.collect(&ast)?;
    c.emit_program(&ast)?;
    Ok(assemble(c.fns, c.globals, c.exported_fns))
}

/// Lower the compiler IR to the flat byte encoding the VM executes in
/// place: two passes per function — measure each instruction's byte offset,
/// then emit with jump targets mapped from instruction indices to byte
/// offsets. Per-instruction positions collapse to offset-keyed runs.
fn assemble(fns: Vec<FnIr>, globals: Vec<GlobalDef>, exported_fns: Vec<(String, u16)>) -> Program {
    let mut code: Vec<u8> = Vec::new();
    let mut defs: Vec<FnDef> = Vec::with_capacity(fns.len());
    for f in fns {
        // pass 1: byte offset of each instruction (+ end)
        let mut offsets: Vec<u32> = Vec::with_capacity(f.code.len() + 1);
        let mut at = 0u32;
        for insn in &f.code {
            offsets.push(at);
            at += insn_len(insn);
        }
        offsets.push(at);
        // pass 2: emit
        let code_start = code.len() as u32;
        for insn in &f.code {
            emit_insn(&mut code, insn, &offsets);
        }
        let code_len = code.len() as u32 - code_start;
        // positions → offset-keyed runs (statement granularity ⇒ few runs)
        let mut pos: Vec<(u32, u32, u32)> = Vec::new();
        for (i, &(line, col)) in f.pos.iter().enumerate() {
            match pos.last() {
                Some(&(_, l, c)) if l == line && c == col => {}
                _ => pos.push((offsets[i], line, col)),
            }
        }
        defs.push(FnDef {
            name: f.name,
            params: f.params,
            locals: f.local_names.len() as u8,
            code_start,
            code_len,
            pos,
            local_names: f.local_names,
        });
    }
    Program {
        code,
        fns: defs,
        globals,
        exported_fns,
        pixel_count_g: 0,
    }
}

/// Encoded byte length of one IR instruction.
fn insn_len(insn: &Insn) -> u32 {
    use Insn::*;
    match insn {
        Const(Value::Num(_)) | Const(Value::Arr(_)) => 5,
        Const(Value::Fun(_)) | Const(Value::Builtin(_)) => 3,
        LoadG(_) | StoreG(_) | NewArray(_) => 3,
        LoadL(_) | StoreL(_) => 2,
        Jmp(_) | JmpIfFalse(_) | JmpIfTruePeek(_) | JmpIfFalsePeek(_) => 5,
        CallFn { .. } | CallBuiltin { .. } => 4,
        CallValue { .. } => 2,
        _ => 1,
    }
}

fn emit_insn(out: &mut Vec<u8>, insn: &Insn, offsets: &[u32]) {
    use Insn::*;
    let target = |t: u32| offsets.get(t as usize).copied().unwrap_or(*offsets.last().unwrap());
    match insn {
        Const(Value::Num(v)) => {
            out.push(op::CONST_NUM);
            out.extend_from_slice(&v.raw().to_le_bytes());
        }
        // Arr constants don't exist in compiler output; encode zero.
        Const(Value::Arr(_)) => {
            out.push(op::CONST_NUM);
            out.extend_from_slice(&0i32.to_le_bytes());
        }
        Const(Value::Fun(i)) => {
            out.push(op::CONST_FUN);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Const(Value::Builtin(b)) => {
            out.push(op::CONST_BUILTIN);
            out.extend_from_slice(&b.to_le_bytes());
        }
        LoadG(i) => {
            out.push(op::LOAD_G);
            out.extend_from_slice(&i.to_le_bytes());
        }
        StoreG(i) => {
            out.push(op::STORE_G);
            out.extend_from_slice(&i.to_le_bytes());
        }
        LoadL(i) => {
            out.push(op::LOAD_L);
            out.push(*i);
        }
        StoreL(i) => {
            out.push(op::STORE_L);
            out.push(*i);
        }
        LoadIdx => out.push(op::LOAD_IDX),
        StoreIdx => out.push(op::STORE_IDX),
        ArrLen => out.push(op::ARR_LEN),
        NewArray(n) => {
            out.push(op::NEW_ARRAY);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Dup => out.push(op::DUP),
        Dup2 => out.push(op::DUP2),
        Pop => out.push(op::POP),
        Add => out.push(op::ADD),
        Sub => out.push(op::SUB),
        Mul => out.push(op::MUL),
        Div => out.push(op::DIV),
        Rem => out.push(op::REM),
        Pow => out.push(op::POW),
        Neg => out.push(op::NEG),
        Not => out.push(op::NOT),
        BitNot => out.push(op::BIT_NOT),
        BitAnd => out.push(op::BIT_AND),
        BitOr => out.push(op::BIT_OR),
        BitXor => out.push(op::BIT_XOR),
        Shl => out.push(op::SHL),
        Shr => out.push(op::SHR),
        Lt => out.push(op::LT),
        Le => out.push(op::LE),
        Gt => out.push(op::GT),
        Ge => out.push(op::GE),
        Eq => out.push(op::EQ),
        Ne => out.push(op::NE),
        Jmp(t) => {
            out.push(op::JMP);
            out.extend_from_slice(&target(*t).to_le_bytes());
        }
        JmpIfFalse(t) => {
            out.push(op::JMP_IF_FALSE);
            out.extend_from_slice(&target(*t).to_le_bytes());
        }
        JmpIfTruePeek(t) => {
            out.push(op::JMP_IF_TRUE_PEEK);
            out.extend_from_slice(&target(*t).to_le_bytes());
        }
        JmpIfFalsePeek(t) => {
            out.push(op::JMP_IF_FALSE_PEEK);
            out.extend_from_slice(&target(*t).to_le_bytes());
        }
        CallFn { fn_idx, argc } => {
            out.push(op::CALL_FN);
            out.extend_from_slice(&fn_idx.to_le_bytes());
            out.push(*argc);
        }
        CallBuiltin { b, argc } => {
            out.push(op::CALL_BUILTIN);
            out.extend_from_slice(&b.to_le_bytes());
            out.push(*argc);
        }
        CallValue { argc } => {
            out.push(op::CALL_VALUE);
            out.push(*argc);
        }
        Ret => out.push(op::RET),
        RetNull => out.push(op::RET_NULL),
    }
}

/// Predefined constants (name, value). `pixelCount` is global 0, written by
/// the engine before init runs. Constants use literal quantization (16.15,
/// truncated) — oracle-confirmed: PI reads back as raw 205886 on hardware.
fn predefined() -> Vec<GlobalDef> {
    use core::f64::consts;
    let g = |name: &str, v: f64| GlobalDef {
        name: name.to_string(),
        export: false,
        init: Fx::from_f64_lit(v),
        predefined: true,
    };
    let gi = |name: &str, v: i32| GlobalDef {
        name: name.to_string(),
        export: false,
        init: Fx::from_int(v),
        predefined: true,
    };
    alloc::vec![
        g("pixelCount", 0.0),
        g("E", consts::E),
        g("PI", consts::PI),
        g("PI2", consts::TAU),
        g("PI3_4", consts::PI * 0.75),
        g("PISQ", consts::PI * consts::PI),
        g("LN2", consts::LN_2),
        g("LN10", consts::LN_10),
        g("LOG2E", consts::LOG2_E),
        g("LOG10E", consts::LOG10_E),
        g("SQRT1_2", consts::FRAC_1_SQRT_2),
        g("SQRT2", consts::SQRT_2),
        // JS-isms the PB compiler accepts; both behave as 0. TODO(oracle).
        gi("null", 0),
        gi("undefined", 0),
        // GPIO constants, oracle-probed from fw 3.67
        gi("LOW", 0),
        gi("HIGH", 1),
        gi("INPUT", 1),
        gi("OUTPUT", 2),
        gi("INPUT_PULLUP", 5),
        gi("INPUT_PULLDOWN", 9),
        gi("OUTPUT_OPEN_DRAIN", 18),
        gi("ANALOG", 192),
    ]
}

/// One function in compiler IR form (see [`Insn`]); [`assemble`] turns the
/// full set into the byte-coded [`Program`].
struct FnIr {
    name: String,
    params: u8,
    code: Vec<Insn>,
    /// (line, col) per instruction — collapsed to runs at assembly.
    pos: Vec<(u32, u32)>,
    /// Local slot names, params first (count = the local slot count).
    local_names: Vec<String>,
}

impl FnIr {
    fn placeholder(name: String) -> FnIr {
        FnIr {
            name,
            params: 0,
            code: Vec::new(),
            pos: Vec::new(),
            local_names: Vec::new(),
        }
    }
}

struct Compiler<'s> {
    src: &'s str,
    globals: Vec<GlobalDef>,
    fns: Vec<FnIr>,
    /// Top-level named functions: (name, fn index, exported).
    named_fns: Vec<(String, u16, bool)>,
    exported_fns: Vec<(String, u16)>,
    /// Function names the pattern assigns to — demoted to plain global
    /// variables holding a function value (JS-style mutable bindings).
    demoted: Vec<String>,
    /// Names declared `const` at the top level (reassignment is an error).
    const_globals: BTreeSet<String>,
    /// Current emit recursion depth — bounded like the parser's so a deep
    /// AST becomes a compile error instead of a stack overflow on the
    /// firmware's small task stack.
    depth: u32,
}

/// Matches the parser's nesting bound (see parse.rs MAX_DEPTH).
const MAX_EMIT_DEPTH: u32 = 60;

enum Place {
    Local(u8),
    Global(u16),
    Func(u16),
    Builtin(u16),
}

impl<'s> Compiler<'s> {
    fn new(src: &'s str) -> Compiler<'s> {
        Compiler {
            src,
            globals: predefined(),
            fns: alloc::vec![FnIr::placeholder("<init>".to_string())],
            named_fns: Vec::new(),
            exported_fns: Vec::new(),
            demoted: Vec::new(),
            const_globals: BTreeSet::new(),
            depth: 0,
        }
    }

    fn global_idx(&self, name: &str) -> Option<u16> {
        self.globals
            .iter()
            .position(|g| g.name == name)
            .map(|i| i as u16)
    }

    fn ensure_global(&mut self, name: &str, export: bool, span: Span) -> Result<u16, Diagnostic> {
        if let Some(i) = self.global_idx(name) {
            if export {
                self.globals[i as usize].export = true;
            }
            return Ok(i);
        }
        if self.globals.len() >= MAX_GLOBALS {
            return Err(Diagnostic::new(
                span,
                format!("too many globals (max {MAX_GLOBALS})"),
            ));
        }
        self.globals.push(GlobalDef {
            name: name.to_string(),
            export,
            init: Fx::ZERO,
            predefined: false,
        });
        Ok((self.globals.len() - 1) as u16)
    }

    // ---- pass 1: collect functions and globals ----

    fn collect(&mut self, top: &[Stmt]) -> Result<(), Diagnostic> {
        // Register named functions first so calls resolve in any order.
        // PB flattens ALL function declarations to global scope regardless of
        // nesting (corpus patterns call functions declared inside other
        // functions), and duplicates are allowed — the last definition wins.
        register_fns(self, top);
        // reserve placeholder defs so indices are stable during emission
        for (name, _, _) in self.named_fns.clone() {
            self.fns.push(FnIr::placeholder(name));
        }
        // top level: every var / assignment is a global (even inside blocks)
        let empty: Vec<String> = Vec::new();
        for s in top {
            match &s.kind {
                StmtKind::Func { params, body, .. } => {
                    let locals = function_scope(params, body);
                    self.scan_stmts(body, &locals, false)?;
                }
                _ => self.scan_stmt(s, &empty, true)?,
            }
        }
        Ok(())
    }

    fn scan_stmts(
        &mut self,
        stmts: &[Stmt],
        locals: &[String],
        top: bool,
    ) -> Result<(), Diagnostic> {
        for s in stmts {
            self.scan_stmt(s, locals, top)?;
        }
        Ok(())
    }

    fn scan_stmt(&mut self, s: &Stmt, locals: &[String], top: bool) -> Result<(), Diagnostic> {
        match &s.kind {
            // nested named function: its own scope, like a lambda (the name
            // becomes a hoisted local of the enclosing function)
            StmtKind::Func { params, body, .. } => {
                let scope = function_scope(params, body);
                self.scan_stmts(body, &scope, false)
            }
            StmtKind::Var {
                export,
                kind,
                decls,
            } => {
                for d in decls {
                    if top {
                        self.ensure_global(&d.name, *export, d.span)?;
                        if *kind == DeclKind::Const {
                            self.const_globals.insert(d.name.clone());
                        }
                    }
                    if let Some(init) = &d.init {
                        self.scan_expr(init, locals)?;
                    }
                }
                Ok(())
            }
            StmtKind::Expr(e) => self.scan_expr(e, locals),
            StmtKind::If { cond, then, els } => {
                self.scan_expr(cond, locals)?;
                self.scan_stmt(then, locals, top)?;
                if let Some(e) = els {
                    self.scan_stmt(e, locals, top)?;
                }
                Ok(())
            }
            StmtKind::While { cond, body } => {
                self.scan_expr(cond, locals)?;
                self.scan_stmt(body, locals, top)
            }
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.scan_stmt(i, locals, top)?;
                }
                if let Some(c) = cond {
                    self.scan_expr(c, locals)?;
                }
                if let Some(u) = update {
                    self.scan_expr(u, locals)?;
                }
                self.scan_stmt(body, locals, top)
            }
            StmtKind::Block(b) => self.scan_stmts(b, locals, top),
            StmtKind::Return(Some(e)) => self.scan_expr(e, locals),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Empty => {
                Ok(())
            }
        }
    }

    fn scan_expr(&mut self, e: &Expr, locals: &[String]) -> Result<(), Diagnostic> {
        match &e.kind {
            ExprKind::Num(_) => Ok(()),
            ExprKind::Ident(_) => Ok(()), // reads checked during emission
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    self.scan_expr(el, locals)?;
                }
                Ok(())
            }
            ExprKind::Unary { expr, .. } => self.scan_expr(expr, locals),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.scan_expr(lhs, locals)?;
                self.scan_expr(rhs, locals)
            }
            ExprKind::Assign { target, value, .. } => {
                self.scan_assign_target(target, locals)?;
                self.scan_expr(value, locals)
            }
            ExprKind::IncDec { target, .. } => self.scan_assign_target(target, locals),
            ExprKind::Ternary { cond, then, els } => {
                self.scan_expr(cond, locals)?;
                self.scan_expr(then, locals)?;
                self.scan_expr(els, locals)
            }
            ExprKind::Call { callee, args } => {
                self.scan_expr(callee, locals)?;
                for a in args {
                    self.scan_expr(a, locals)?;
                }
                Ok(())
            }
            ExprKind::Index { obj, index } => {
                self.scan_expr(obj, locals)?;
                self.scan_expr(index, locals)
            }
            ExprKind::Member { obj, .. } => self.scan_expr(obj, locals),
            ExprKind::Lambda { params, body } => match body {
                LambdaBody::Expr(e) => {
                    let scope = function_scope(params, &[]);
                    self.scan_expr(e, &scope)
                }
                LambdaBody::Block(stmts) => {
                    let scope = function_scope(params, stmts);
                    self.scan_stmts(stmts, &scope, false)
                }
            },
        }
    }

    fn scan_assign_target(&mut self, target: &Expr, locals: &[String]) -> Result<(), Diagnostic> {
        match &target.kind {
            ExprKind::Ident(name) => {
                let is_local = locals.iter().any(|l| l == name);
                if is_local {
                    return Ok(());
                }
                // assigning to a function name demotes it to a plain global
                // variable initialized with the function value (JS-style)
                if self.named_fns.iter().any(|(n, _, _)| n == name)
                    && !self.demoted.iter().any(|n| n == name)
                {
                    self.demoted.push(name.clone());
                }
                // implicit assignment creates a global
                self.ensure_global(name, false, target.span)?;
                Ok(())
            }
            ExprKind::Index { obj, index } => {
                self.scan_expr(obj, locals)?;
                self.scan_expr(index, locals)
            }
            _ => Ok(()),
        }
    }

    // ---- pass 2: emission ----

    fn emit_program(&mut self, top: &[Stmt]) -> Result<(), Diagnostic> {
        // All function declarations (any nesting depth) compile into their
        // registered global slots; duplicates emit in walk order, last wins.
        let mut decls: Vec<&Stmt> = Vec::new();
        walk_fns(top, &mut |s| decls.push(s));
        for s in decls {
            let StmtKind::Func {
                export,
                name,
                params,
                body,
            } = &s.kind
            else {
                unreachable!()
            };
            let idx = self
                .named_fns
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|&(_, i, _)| i)
                .expect("registered in collect");
            let def = self.emit_function(name.clone(), params, body, s.span)?;
            self.fns[idx as usize] = def;
            if *export && !self.exported_fns.iter().any(|(n, _)| n == name) {
                self.exported_fns.push((name.clone(), idx));
            }
        }

        // top-level init (fn 0). Demoted functions (ones the pattern assigns
        // to, making them plain variables) get their global slots initialized
        // first.
        let mut ctx = FnCtx::new(Vec::new(), true);
        for name in self.demoted.clone() {
            let idx = self
                .named_fns
                .iter()
                .find(|(n, _, _)| *n == name)
                .map(|&(_, i, _)| i)
                .expect("demoted implies registered");
            let g = self.ensure_global(&name, false, Span::default())?;
            ctx.push(Insn::Const(Value::Fun(idx)));
            ctx.push(Insn::StoreG(g));
            ctx.push(Insn::Pop);
        }
        for s in top {
            if matches!(s.kind, StmtKind::Func { .. }) {
                continue;
            }
            self.emit_stmt(&mut ctx, s)?;
        }
        ctx.push(Insn::RetNull);
        self.fns[0].pos = ctx.pos; // keep line info for init-time vmerrs
        self.fns[0].code = ctx.code;
        Ok(())
    }

    fn emit_function(
        &mut self,
        name: String,
        params: &[String],
        body: &[Stmt],
        span: Span,
    ) -> Result<FnIr, Diagnostic> {
        let locals = function_scope(params, body);
        if locals.len() > MAX_LOCALS {
            return Err(Diagnostic::new(
                span,
                format!("too many locals in `{name}` (max {MAX_LOCALS})"),
            ));
        }
        let mut ctx = FnCtx::new(locals, false);
        for s in body {
            self.emit_stmt(&mut ctx, s)?;
        }
        ctx.push(Insn::RetNull);
        Ok(ctx.finish(name, params.len() as u8))
    }

    fn emit_lambda(
        &mut self,
        params: &[String],
        body: &LambdaBody,
        span: Span,
    ) -> Result<u16, Diagnostic> {
        let idx = self.fns.len() as u16;
        let name = format!("<lambda#{idx}>");
        // reserve slot first (nested lambdas may allocate more)
        self.fns.push(FnIr::placeholder(name.clone()));
        let def = match body {
            LambdaBody::Expr(e) => {
                let locals = function_scope(params, &[]);
                let mut ctx = FnCtx::new(locals, false);
                ctx.set_pos(line_col(self.src, e.span.start));
                self.emit_expr(&mut ctx, e)?;
                ctx.push(Insn::Ret);
                ctx.finish(name.clone(), params.len() as u8)
            }
            LambdaBody::Block(stmts) => {
                let locals = function_scope(params, stmts);
                if locals.len() > MAX_LOCALS {
                    return Err(Diagnostic::new(
                        span,
                        "too many locals in lambda".to_string(),
                    ));
                }
                let mut ctx = FnCtx::new(locals, false);
                for s in stmts {
                    self.emit_stmt(&mut ctx, s)?;
                }
                ctx.push(Insn::RetNull);
                ctx.finish(name, params.len() as u8)
            }
        };
        self.fns[idx as usize] = def;
        Ok(idx)
    }

    fn resolve(&self, ctx: &FnCtx, name: &str, span: Span) -> Result<Place, Diagnostic> {
        if let Some(i) = ctx.locals.iter().position(|l| l == name) {
            return Ok(Place::Local(i as u8));
        }
        if !self.demoted.iter().any(|n| n == name) {
            if let Some(&(_, idx, _)) = self.named_fns.iter().find(|(n, _, _)| n == name) {
                return Ok(Place::Func(idx));
            }
        }
        if let Some(i) = self.global_idx(name) {
            return Ok(Place::Global(i));
        }
        if let Some(b) = lookup_builtin(name) {
            return Ok(Place::Builtin(b));
        }
        Err(Diagnostic::new(
            span,
            format!("unknown identifier `{name}`"),
        ))
    }

    fn emit_stmt(&mut self, ctx: &mut FnCtx, s: &Stmt) -> Result<(), Diagnostic> {
        self.depth += 1;
        if self.depth > MAX_EMIT_DEPTH {
            self.depth -= 1;
            return Err(Diagnostic::new(s.span, "statement nesting too deep"));
        }
        let r = self.emit_stmt_inner(ctx, s);
        self.depth -= 1;
        r
    }

    fn emit_stmt_inner(&mut self, ctx: &mut FnCtx, s: &Stmt) -> Result<(), Diagnostic> {
        ctx.set_pos(line_col(self.src, s.span.start));
        match &s.kind {
            StmtKind::Empty => Ok(()),
            // nested named functions were already bound at function entry
            StmtKind::Func { .. } => Ok(()),
            StmtKind::Var { decls, kind, .. } => {
                for d in decls {
                    if let Some(init) = &d.init {
                        self.emit_expr(ctx, init)?;
                        // a declaration's own store is always allowed; it also
                        // registers const-ness so later assignments error
                        self.emit_store_decl(ctx, &d.name, d.span, *kind == DeclKind::Const)?;
                        ctx.push(Insn::Pop);
                    }
                }
                Ok(())
            }
            StmtKind::Expr(e) => {
                self.emit_expr(ctx, e)?;
                ctx.push(Insn::Pop);
                Ok(())
            }
            StmtKind::If { cond, then, els } => {
                self.emit_expr(ctx, cond)?;
                let jf = ctx.emit_placeholder();
                self.emit_stmt(ctx, then)?;
                if let Some(els) = els {
                    let jend = ctx.emit_placeholder();
                    ctx.patch(jf, Insn::JmpIfFalse(ctx.here()));
                    self.emit_stmt(ctx, els)?;
                    ctx.patch(jend, Insn::Jmp(ctx.here()));
                } else {
                    ctx.patch(jf, Insn::JmpIfFalse(ctx.here()));
                }
                Ok(())
            }
            StmtKind::While { cond, body } => {
                let start = ctx.here();
                self.emit_expr(ctx, cond)?;
                let jf = ctx.emit_placeholder();
                ctx.loops.push(LoopFrame::default());
                self.emit_stmt(ctx, body)?;
                ctx.push(Insn::Jmp(start));
                let frame = ctx.loops.pop().unwrap();
                let end = ctx.here();
                ctx.patch(jf, Insn::JmpIfFalse(end));
                for b in frame.breaks {
                    ctx.patch(b, Insn::Jmp(end));
                }
                for c in frame.continues {
                    ctx.patch(c, Insn::Jmp(start));
                }
                Ok(())
            }
            StmtKind::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init {
                    self.emit_stmt(ctx, i)?;
                }
                let start = ctx.here();
                let jf = if let Some(c) = cond {
                    self.emit_expr(ctx, c)?;
                    Some(ctx.emit_placeholder())
                } else {
                    None
                };
                ctx.loops.push(LoopFrame::default());
                self.emit_stmt(ctx, body)?;
                let frame = ctx.loops.pop().unwrap();
                let cont = ctx.here();
                if let Some(u) = update {
                    self.emit_expr(ctx, u)?;
                    ctx.push(Insn::Pop);
                }
                ctx.push(Insn::Jmp(start));
                let end = ctx.here();
                if let Some(jf) = jf {
                    ctx.patch(jf, Insn::JmpIfFalse(end));
                }
                for b in frame.breaks {
                    ctx.patch(b, Insn::Jmp(end));
                }
                for c in frame.continues {
                    ctx.patch(c, Insn::Jmp(cont));
                }
                Ok(())
            }
            StmtKind::Block(body) => {
                for s in body {
                    self.emit_stmt(ctx, s)?;
                }
                Ok(())
            }
            StmtKind::Return(v) => {
                match v {
                    Some(e) => {
                        self.emit_expr(ctx, e)?;
                        ctx.push(Insn::Ret);
                    }
                    None => ctx.push(Insn::RetNull),
                }
                Ok(())
            }
            StmtKind::Break => {
                let p = ctx.emit_placeholder();
                match ctx.loops.last_mut() {
                    Some(f) => {
                        f.breaks.push(p);
                        Ok(())
                    }
                    None => Err(Diagnostic::new(
                        s.span,
                        "`break` outside a loop".to_string(),
                    )),
                }
            }
            StmtKind::Continue => {
                let p = ctx.emit_placeholder();
                match ctx.loops.last_mut() {
                    Some(f) => {
                        f.continues.push(p);
                        Ok(())
                    }
                    None => Err(Diagnostic::new(
                        s.span,
                        "`continue` outside a loop".to_string(),
                    )),
                }
            }
        }
    }

    /// Emit a store to a named variable (value on stack; leaves it there).
    /// Rejects reassignment of a `const`.
    fn emit_store(&mut self, ctx: &mut FnCtx, name: &str, span: Span) -> Result<(), Diagnostic> {
        match self.resolve(ctx, name, span)? {
            Place::Local(i) => {
                if ctx.const_locals.contains(&i) {
                    return Err(Diagnostic::new(
                        span,
                        format!("cannot assign to `{name}` — it is declared const"),
                    ));
                }
                ctx.push(Insn::StoreL(i));
            }
            Place::Global(i) => {
                if self.const_globals.contains(name) {
                    return Err(Diagnostic::new(
                        span,
                        format!("cannot assign to `{name}` — it is declared const"),
                    ));
                }
                ctx.push(Insn::StoreG(i));
            }
            Place::Func(_) | Place::Builtin(_) => {
                return Err(Diagnostic::new(span, format!("cannot assign to `{name}`")))
            }
        }
        Ok(())
    }

    /// Emit the store for a declaration's initializer. Bypasses the const
    /// check (a const's own init is legal) and, for const, records the
    /// binding so subsequent assignments are rejected.
    fn emit_store_decl(
        &mut self,
        ctx: &mut FnCtx,
        name: &str,
        span: Span,
        is_const: bool,
    ) -> Result<(), Diagnostic> {
        match self.resolve(ctx, name, span)? {
            Place::Local(i) => {
                ctx.push(Insn::StoreL(i));
                if is_const {
                    ctx.const_locals.insert(i);
                }
            }
            Place::Global(i) => {
                ctx.push(Insn::StoreG(i));
                // const globals already recorded during scanning
            }
            Place::Func(_) | Place::Builtin(_) => {
                return Err(Diagnostic::new(span, format!("cannot assign to `{name}`")))
            }
        }
        Ok(())
    }

    fn emit_expr(&mut self, ctx: &mut FnCtx, e: &Expr) -> Result<(), Diagnostic> {
        self.depth += 1;
        if self.depth > MAX_EMIT_DEPTH {
            self.depth -= 1;
            return Err(Diagnostic::new(e.span, "expression nesting too deep"));
        }
        let r = self.emit_expr_inner(ctx, e);
        self.depth -= 1;
        r
    }

    fn emit_expr_inner(&mut self, ctx: &mut FnCtx, e: &Expr) -> Result<(), Diagnostic> {
        match &e.kind {
            ExprKind::Num(v) => {
                ctx.push(Insn::Const(Value::Num(*v)));
                Ok(())
            }
            ExprKind::Ident(name) => {
                match self.resolve(ctx, name, e.span)? {
                    Place::Local(i) => ctx.push(Insn::LoadL(i)),
                    Place::Global(i) => ctx.push(Insn::LoadG(i)),
                    Place::Func(i) => ctx.push(Insn::Const(Value::Fun(i))),
                    Place::Builtin(b) => ctx.push(Insn::Const(Value::Builtin(b))),
                }
                Ok(())
            }
            ExprKind::ArrayLit(elems) => {
                if elems.len() > u16::MAX as usize {
                    return Err(Diagnostic::new(
                        e.span,
                        "array literal too large".to_string(),
                    ));
                }
                for el in elems {
                    self.emit_expr(ctx, el)?;
                }
                ctx.push(Insn::NewArray(elems.len() as u16));
                Ok(())
            }
            ExprKind::Unary { op, expr } => {
                self.emit_expr(ctx, expr)?;
                match op {
                    UnOp::Neg => ctx.push(Insn::Neg),
                    UnOp::Pos => {} // numeric identity
                    UnOp::Not => ctx.push(Insn::Not),
                    UnOp::BitNot => ctx.push(Insn::BitNot),
                }
                Ok(())
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::And => {
                    self.emit_expr(ctx, lhs)?;
                    let j = ctx.emit_placeholder();
                    ctx.push(Insn::Pop);
                    self.emit_expr(ctx, rhs)?;
                    ctx.patch(j, Insn::JmpIfFalsePeek(ctx.here()));
                    Ok(())
                }
                BinOp::Or => {
                    self.emit_expr(ctx, lhs)?;
                    let j = ctx.emit_placeholder();
                    ctx.push(Insn::Pop);
                    self.emit_expr(ctx, rhs)?;
                    ctx.patch(j, Insn::JmpIfTruePeek(ctx.here()));
                    Ok(())
                }
                _ => {
                    self.emit_expr(ctx, lhs)?;
                    self.emit_expr(ctx, rhs)?;
                    ctx.push(bin_insn(*op));
                    Ok(())
                }
            },
            ExprKind::Assign { op, target, value } => match &target.kind {
                ExprKind::Ident(name) => {
                    if let Some(op) = op {
                        self.emit_expr(ctx, target)?;
                        self.emit_expr(ctx, value)?;
                        ctx.push(bin_insn(*op));
                    } else {
                        self.emit_expr(ctx, value)?;
                    }
                    self.emit_store(ctx, name, target.span)
                }
                ExprKind::Index { obj, index } => {
                    self.emit_expr(ctx, obj)?;
                    self.emit_expr(ctx, index)?;
                    if let Some(op) = op {
                        ctx.push(Insn::Dup2);
                        ctx.push(Insn::LoadIdx);
                        self.emit_expr(ctx, value)?;
                        ctx.push(bin_insn(*op));
                    } else {
                        self.emit_expr(ctx, value)?;
                    }
                    ctx.push(Insn::StoreIdx);
                    Ok(())
                }
                _ => Err(Diagnostic::new(
                    target.span,
                    "invalid assignment target".to_string(),
                )),
            },
            ExprKind::IncDec {
                inc,
                prefix,
                target,
            } => {
                let one = Insn::Const(Value::Num(Fx::ONE));
                let (fwd, inv) = if *inc {
                    (Insn::Add, Insn::Sub)
                } else {
                    (Insn::Sub, Insn::Add)
                };
                match &target.kind {
                    ExprKind::Ident(name) => {
                        self.emit_expr(ctx, target)?;
                        ctx.push(one);
                        ctx.push(fwd);
                        self.emit_store(ctx, name, target.span)?;
                    }
                    ExprKind::Index { obj, index } => {
                        self.emit_expr(ctx, obj)?;
                        self.emit_expr(ctx, index)?;
                        ctx.push(Insn::Dup2);
                        ctx.push(Insn::LoadIdx);
                        ctx.push(one);
                        ctx.push(fwd);
                        ctx.push(Insn::StoreIdx);
                    }
                    _ => {
                        return Err(Diagnostic::new(
                            target.span,
                            "invalid increment target".to_string(),
                        ))
                    }
                }
                if !prefix {
                    // stack holds the NEW value; recover the old one (exact
                    // inverse under wrapping arithmetic)
                    ctx.push(one);
                    ctx.push(inv);
                }
                Ok(())
            }
            ExprKind::Ternary { cond, then, els } => {
                self.emit_expr(ctx, cond)?;
                let jf = ctx.emit_placeholder();
                self.emit_expr(ctx, then)?;
                let jend = ctx.emit_placeholder();
                ctx.patch(jf, Insn::JmpIfFalse(ctx.here()));
                self.emit_expr(ctx, els)?;
                ctx.patch(jend, Insn::Jmp(ctx.here()));
                Ok(())
            }
            ExprKind::Call { callee, args } => {
                if args.len() > 15 {
                    return Err(Diagnostic::new(e.span, "too many arguments".to_string()));
                }
                let argc = args.len() as u8;
                // method form: a.mutate(f) → arrayMutate(a, f)
                if let ExprKind::Member { obj, name } = &callee.kind {
                    let Some(b) = lookup_method(name) else {
                        return Err(Diagnostic::new(
                            callee.span,
                            format!("unknown method `.{name}()`"),
                        ));
                    };
                    self.emit_expr(ctx, obj)?;
                    for a in args {
                        self.emit_expr(ctx, a)?;
                    }
                    ctx.push(Insn::CallBuiltin { b, argc: argc + 1 });
                    return Ok(());
                }
                // direct call of a named function or builtin
                if let ExprKind::Ident(name) = &callee.kind {
                    match self.resolve(ctx, name, callee.span)? {
                        Place::Func(fn_idx) => {
                            for a in args {
                                self.emit_expr(ctx, a)?;
                            }
                            ctx.push(Insn::CallFn { fn_idx, argc });
                            return Ok(());
                        }
                        Place::Builtin(b) => {
                            for a in args {
                                self.emit_expr(ctx, a)?;
                            }
                            ctx.push(Insn::CallBuiltin { b, argc });
                            return Ok(());
                        }
                        _ => {} // fall through to value call
                    }
                }
                self.emit_expr(ctx, callee)?;
                for a in args {
                    self.emit_expr(ctx, a)?;
                }
                ctx.push(Insn::CallValue { argc });
                Ok(())
            }
            ExprKind::Index { obj, index } => {
                self.emit_expr(ctx, obj)?;
                self.emit_expr(ctx, index)?;
                ctx.push(Insn::LoadIdx);
                Ok(())
            }
            ExprKind::Member { obj, name } => {
                if name == "length" {
                    self.emit_expr(ctx, obj)?;
                    ctx.push(Insn::ArrLen);
                    Ok(())
                } else {
                    Err(Diagnostic::new(
                        e.span,
                        format!("unknown property `.{name}`"),
                    ))
                }
            }
            ExprKind::Lambda { params, body } => {
                let idx = self.emit_lambda(params, body, e.span)?;
                ctx.push(Insn::Const(Value::Fun(idx)));
                Ok(())
            }
        }
    }
}

fn bin_insn(op: BinOp) -> Insn {
    match op {
        BinOp::Add => Insn::Add,
        BinOp::Sub => Insn::Sub,
        BinOp::Mul => Insn::Mul,
        BinOp::Div => Insn::Div,
        BinOp::Rem => Insn::Rem,
        BinOp::Pow => Insn::Pow,
        BinOp::Shl => Insn::Shl,
        BinOp::Shr => Insn::Shr,
        BinOp::BitAnd => Insn::BitAnd,
        BinOp::BitOr => Insn::BitOr,
        BinOp::BitXor => Insn::BitXor,
        BinOp::Lt => Insn::Lt,
        BinOp::Le => Insn::Le,
        BinOp::Gt => Insn::Gt,
        BinOp::Ge => Insn::Ge,
        BinOp::Eq => Insn::Eq,
        BinOp::Ne => Insn::Ne,
        BinOp::And | BinOp::Or => unreachable!("short-circuit ops emitted separately"),
    }
}

struct FnCtx {
    locals: Vec<String>,
    code: Vec<Insn>,
    /// (line, col) per emitted instruction — statement granularity.
    pos: Vec<(u32, u32)>,
    cur_pos: (u32, u32),
    loops: Vec<LoopFrame>,
    /// Local slots declared `const` in this function.
    const_locals: BTreeSet<u8>,
    #[allow(dead_code)]
    is_top: bool,
}

#[derive(Default)]
struct LoopFrame {
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

impl FnCtx {
    fn new(locals: Vec<String>, is_top: bool) -> FnCtx {
        FnCtx {
            locals,
            code: Vec::new(),
            pos: Vec::new(),
            cur_pos: (0, 0),
            loops: Vec::new(),
            const_locals: BTreeSet::new(),
            is_top,
        }
    }

    fn set_pos(&mut self, pos: (u32, u32)) {
        self.cur_pos = pos;
    }

    fn push(&mut self, insn: Insn) {
        self.code.push(insn);
        self.pos.push(self.cur_pos);
    }

    fn here(&self) -> u32 {
        self.code.len() as u32
    }

    fn emit_placeholder(&mut self) -> usize {
        self.push(Insn::Jmp(u32::MAX));
        self.code.len() - 1
    }

    fn patch(&mut self, at: usize, insn: Insn) {
        self.code[at] = insn;
    }

    fn finish(self, name: String, params: u8) -> FnIr {
        FnIr {
            name,
            params,
            code: self.code,
            pos: self.pos,
            local_names: self.locals,
        }
    }
}

/// Walk every statement (including inside function bodies — PB flattens all
/// declarations to global scope) and visit each `function` declaration.
fn walk_fns<'a>(stmts: &'a [Stmt], f: &mut impl FnMut(&'a Stmt)) {
    for s in stmts {
        match &s.kind {
            StmtKind::Func { body, .. } => {
                f(s);
                walk_fns(body, f);
            }
            StmtKind::If { then, els, .. } => {
                walk_fns(core::slice::from_ref(then), f);
                if let Some(e) = els {
                    walk_fns(core::slice::from_ref(e), f);
                }
            }
            StmtKind::While { body, .. } => walk_fns(core::slice::from_ref(body), f),
            StmtKind::For { init, body, .. } => {
                if let Some(i) = init {
                    walk_fns(core::slice::from_ref(i), f);
                }
                walk_fns(core::slice::from_ref(body), f);
            }
            StmtKind::Block(b) => walk_fns(b, f),
            _ => {}
        }
    }
}

fn register_fns(c: &mut Compiler, top: &[Stmt]) {
    let mut found: Vec<(String, bool)> = Vec::new();
    walk_fns(top, &mut |s| {
        if let StmtKind::Func { export, name, .. } = &s.kind {
            if let Some(e) = found.iter_mut().find(|(n, _)| n == name) {
                e.1 |= *export;
            } else {
                found.push((name.clone(), *export));
            }
        }
    });
    for (name, export) in found {
        let idx = (c.fns.len() + c.named_fns.len()) as u16;
        c.named_fns.push((name, idx, export));
    }
}

/// A function's local slots: params first, then every hoisted `var` in the
/// body (JS var hoisting — blocks and loops don't scope, lambdas do).
fn function_scope(params: &[String], body: &[Stmt]) -> Vec<String> {
    let mut locals: Vec<String> = params.to_vec();
    hoist(body, &mut locals);
    locals
}

fn hoist(stmts: &[Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match &s.kind {
            StmtKind::Var { decls, .. } => {
                for d in decls {
                    if !out.iter().any(|n| n == &d.name) {
                        out.push(d.name.clone());
                    }
                }
            }
            // note: nested function declarations do NOT hoist as locals —
            // PB flattens them to global scope (register_fns handles them)
            StmtKind::If { then, els, .. } => {
                hoist(core::slice::from_ref(then), out);
                if let Some(e) = els {
                    hoist(core::slice::from_ref(e), out);
                }
            }
            StmtKind::While { body, .. } => hoist(core::slice::from_ref(body), out),
            StmtKind::For { init, body, .. } => {
                if let Some(i) = init {
                    hoist(core::slice::from_ref(i), out);
                }
                hoist(core::slice::from_ref(body), out);
            }
            StmtKind::Block(b) => hoist(b, out),
            _ => {}
        }
    }
}
