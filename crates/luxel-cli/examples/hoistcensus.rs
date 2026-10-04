//! Loop-invariant-hoisting census: how much per-pixel work in `render*`
//! is FRAME-INVARIANT and could be lifted out by the compiler?
//!
//! Run: cargo run --release -p luxel-cli --example hoistcensus -- library
//! (a directory — its top-level `*.js`, not `scenes/`/`sprites/` — or files;
//! `--sites` first prints the source text of every site found.)
//!
//! Pure AST analysis over `luxel_core::parse::parse_program`; nothing is
//! compiled. A HOISTABLE SITE is a maximal subexpression E inside the body
//! of render/render2D/render3D (no inlining) such that
//!  (a) every leaf is a numeric literal, a frozen predefined constant
//!      (`PI`, `PI2`, `E`, … — the compiler's `frozen_globals` set: not
//!      assigned anywhere, `pixelCount` excluded), or a global that is
//!      numeric (never indexed, never `.member`-accessed, never given an
//!      array literal / `array(…)`) and never assigned inside render* or
//!      any user function reachable from render* (a reference to a
//!      function name anywhere in a reachable body is an edge, so function
//!      values passed to builtins count as reachable);
//!  (b) only arithmetic / bitwise / comparison / logical / ternary
//!      operators;
//!  (c) only calls to the pure builtins in `PURE` (and not shadowed by a
//!      user function or local);
//!  (d) references at least one such global (`pixelCount` counts as one);
//!  (e) "valuable" = holds a builtin call or a `*` `/` `%` `**`; an
//!      invariant site with some operator but neither is "trivial".
//! Conservative readings: no dataflow through locals (a render-local is
//! never a leaf, even if assigned an invariant value); no reassociation
//! (`g * x * h` hoists nothing); a name declared anywhere in a function
//! (params, `var`s, lambda params) is a local throughout that function;
//! an undeclared, non-predefined name is not a leaf. Lambda bodies inside
//! render* are walked as part of render*.
//!
//! The inlining estimate: for every direct call `f(args)` from a render*
//! body to a user function, sites in `f`'s body are found with `f`'s
//! params treated as leaves when the call site's argument for that param
//! is invariant — STRICT: a literal/constant-only argument (or a missing
//! one, which is 0); LOOSE: any invariant argument (literals, constants
//! and the globals above). A param `f` writes is never a leaf. One level
//! only; counted per call site; calls from one render* entry to another
//! are skipped (the callee is already counted as an entry). Sites are
//! summed over ALL render* entries a pattern defines, though only one
//! runs on a given map (a `render` 1D fallback beside `render2D` counts).

use luxel_core::ast::{BinOp, Expr, ExprKind, LambdaBody, Stmt, StmtKind};
use luxel_core::parse::parse_program;
use luxel_core::vm::{BKind, BUILTINS};
use std::collections::{BTreeMap, BTreeSet};

const PURE: &[&str] = &[
    "sin", "cos", "tan", "sqrt", "pow", "exp", "log", "log2", "abs", "floor", "ceil", "round",
    "trunc", "frac", "fract", "clamp", "min", "max", "mod", "mix", "lerp", "wave", "triangle",
    "square", "hypot", "hypot3", "length", "length3", "atan", "atan2", "asin", "acos", "sign",
    "step", "smoothstep", "map", "hash", "hash2", "dot", "dot3", "simplex2", "simplex3",
    "saturate", "time",
];

const PREDEF_CONSTS: &[&str] = &[
    "E", "PI", "PI2", "PI3_4", "PISQ", "LN2", "LN10", "LOG2E", "LOG10E", "SQRT1_2", "SQRT2",
    "null", "undefined", "LOW", "HIGH", "INPUT", "OUTPUT", "INPUT_PULLUP", "INPUT_PULLDOWN",
    "OUTPUT_OPEN_DRAIN", "ANALOG",
];

const ENTRIES: &[&str] = &["render", "render2D", "render3D"];

struct Func<'a> {
    params: &'a [String],
    body: &'a [Stmt],
}

/// Whole-program facts the invariance test needs.
struct Prog<'a> {
    funcs: BTreeMap<String, Func<'a>>,
    /// Usable invariant globals: declared/assigned at top level, numeric,
    /// not written by any function reachable from render*.
    inv_globals: BTreeSet<String>,
    /// Predefined constants still frozen in this pattern.
    consts: BTreeSet<String>,
    builtins: BTreeSet<&'static str>,
    /// Source text, for `--sites` (print every site found).
    src: &'a str,
    verbose: bool,
}

/// Per-function scope for one walk.
struct Scope<'a> {
    locals: BTreeSet<String>,
    /// Locals (params) that count as invariant leaves (inline analysis).
    inv_params: &'a BTreeSet<String>,
    /// Label for `--sites` output.
    tag: &'a str,
}

#[derive(Default, Clone, Copy)]
struct Info {
    globals: usize,
    params: usize,
    builtins: usize,
    heavy_op: bool,
    any_op: bool,
}

impl Info {
    fn join(&mut self, o: Info) {
        self.globals += o.globals;
        self.params += o.params;
        self.builtins += o.builtins;
        self.heavy_op |= o.heavy_op;
        self.any_op |= o.any_op;
    }
}

#[derive(Default, Clone, Copy)]
struct Tally {
    sites: usize,
    site_builtins: usize,
    trivial: usize,
    const_only_builtin_sites: usize,
}

// ---- generic AST walking ----

fn stmt_exprs<'a>(s: &'a Stmt, out: &mut Vec<&'a Expr>, subs: &mut Vec<&'a Stmt>) {
    match &s.kind {
        StmtKind::Var { decls, .. } => out.extend(decls.iter().filter_map(|d| d.init.as_ref())),
        StmtKind::Func { body, .. } => subs.extend(body.iter()),
        StmtKind::Expr(e) => out.push(e),
        StmtKind::If { cond, then, els } => {
            out.push(cond);
            subs.push(then);
            if let Some(e) = els {
                subs.push(e);
            }
        }
        StmtKind::While { cond, body } => {
            out.push(cond);
            subs.push(body);
        }
        StmtKind::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                subs.push(i);
            }
            out.extend(cond.iter());
            out.extend(update.iter());
            subs.push(body);
        }
        StmtKind::Switch { disc, cases } => {
            out.push(disc);
            for c in cases {
                out.extend(c.test.iter());
                subs.extend(c.body.iter());
            }
        }
        StmtKind::Block(b) => subs.extend(b.iter()),
        StmtKind::Return(e) => out.extend(e.iter()),
        StmtKind::Assert { cond, .. } => out.push(cond),
        StmtKind::Break | StmtKind::Continue | StmtKind::Empty => {}
    }
}

fn expr_children(e: &Expr) -> Vec<&Expr> {
    match &e.kind {
        ExprKind::Num(_) | ExprKind::Str(_) | ExprKind::Ident(_) => vec![],
        ExprKind::ArrayLit(v) => v.iter().collect(),
        ExprKind::Unary { expr, .. } => vec![expr],
        ExprKind::Binary { lhs, rhs, .. } => vec![lhs, rhs],
        ExprKind::Assign { target, value, .. } => vec![target, value],
        ExprKind::IncDec { target, .. } => vec![target],
        ExprKind::Ternary { cond, then, els } => vec![cond, then, els],
        ExprKind::Call { callee, args } => {
            let mut v = vec![&**callee];
            v.extend(args.iter());
            v
        }
        ExprKind::Index { obj, index } => vec![obj, index],
        ExprKind::Member { obj, .. } => vec![obj],
        ExprKind::Lambda { body, .. } => match body {
            LambdaBody::Expr(x) => vec![x],
            LambdaBody::Block(_) => vec![], // statements: see `visit_expr`
        },
    }
}

/// Visit every expression node (pre-order) under a statement list,
/// including lambda block bodies.
fn visit_stmts<'a>(ss: &'a [Stmt], f: &mut dyn FnMut(&'a Expr)) {
    for s in ss {
        let (mut es, mut subs) = (Vec::new(), Vec::new());
        stmt_exprs(s, &mut es, &mut subs);
        for e in es {
            visit_expr(e, f);
        }
        for sub in subs {
            visit_stmts(std::slice::from_ref(sub), f);
        }
    }
}

fn visit_expr<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    f(e);
    if let ExprKind::Lambda {
        body: LambdaBody::Block(b),
        ..
    } = &e.kind
    {
        visit_stmts(b, f);
    }
    for c in expr_children(e) {
        visit_expr(c, f);
    }
}

/// Every name local to a function: params, `var`/`let`/`const` anywhere in
/// the body, lambda params and their vars.
fn locals_of(params: &[String], body: &[Stmt]) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = params.iter().cloned().collect();
    fn stmts(ss: &[Stmt], out: &mut BTreeSet<String>) {
        for s in ss {
            if let StmtKind::Var { decls, .. } = &s.kind {
                out.extend(decls.iter().map(|d| d.name.clone()));
            }
            let (mut es, mut subs) = (Vec::new(), Vec::new());
            stmt_exprs(s, &mut es, &mut subs);
            for sub in subs {
                stmts(std::slice::from_ref(sub), out);
            }
        }
    }
    stmts(body, &mut out);
    visit_stmts(body, &mut |e| {
        if let ExprKind::Lambda { params, body } = &e.kind {
            out.extend(params.iter().cloned());
            if let LambdaBody::Block(b) = body {
                stmts(b, &mut out);
            }
        }
    });
    out
}

/// Names a function body assigns (`=`, `op=`, `++`/`--`) through a bare
/// identifier target.
fn written_names(body: &[Stmt]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    visit_stmts(body, &mut |e| match &e.kind {
        ExprKind::Assign { target, .. } | ExprKind::IncDec { target, .. } => {
            if let ExprKind::Ident(n) = &target.kind {
                out.insert(n.clone());
            }
        }
        _ => {}
    });
    out
}

fn is_array_value(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::ArrayLit(_) => true,
        ExprKind::Call { callee, .. } => {
            matches!(&callee.kind, ExprKind::Ident(n) if n == "array")
        }
        _ => false,
    }
}

// ---- the invariance test ----

impl Prog<'_> {
    /// `Some(info)` iff `e` is frame-invariant under rules (a)–(c).
    fn inv(&self, e: &Expr, sc: &Scope) -> Option<Info> {
        let mut info = Info::default();
        match &e.kind {
            ExprKind::Num(_) => {}
            ExprKind::Ident(n) => {
                if sc.locals.contains(n) {
                    if sc.inv_params.contains(n) {
                        info.params = 1;
                    } else {
                        return None;
                    }
                } else if self.consts.contains(n) {
                } else if self.inv_globals.contains(n) {
                    info.globals = 1;
                } else {
                    return None;
                }
            }
            ExprKind::Unary { expr, .. } => {
                info = self.inv(expr, sc)?;
                info.any_op = true;
            }
            ExprKind::Binary { op, lhs, rhs } => {
                info = self.inv(lhs, sc)?;
                info.join(self.inv(rhs, sc)?);
                info.any_op = true;
                if matches!(op, BinOp::Mul | BinOp::Div | BinOp::Rem | BinOp::Pow) {
                    info.heavy_op = true;
                }
            }
            ExprKind::Ternary { cond, then, els } => {
                info = self.inv(cond, sc)?;
                info.join(self.inv(then, sc)?);
                info.join(self.inv(els, sc)?);
                info.any_op = true;
            }
            ExprKind::Call { callee, args } => {
                let ExprKind::Ident(n) = &callee.kind else {
                    return None;
                };
                if !PURE.contains(&n.as_str())
                    || self.funcs.contains_key(n)
                    || sc.locals.contains(n)
                    || !self.builtins.contains(n.as_str())
                {
                    return None;
                }
                for a in args {
                    info.join(self.inv(a, sc)?);
                }
                info.builtins += 1;
                info.any_op = true;
            }
            _ => return None,
        }
        Some(info)
    }

    /// Find maximal invariant subexpressions under `e`.
    fn sites_expr(&self, e: &Expr, sc: &Scope, t: &mut Tally) {
        if let Some(i) = self.inv(e, sc) {
            let refs = i.globals + i.params > 0;
            let valuable = i.builtins > 0 || i.heavy_op;
            if self.verbose && refs && (valuable || i.any_op) {
                let text = self
                    .src
                    .get(e.span.start as usize..e.span.end as usize)
                    .unwrap_or("?");
                let kind = if valuable { "SITE" } else { "trivial" };
                println!("    [{}] {kind} b={}: {text}", sc.tag, i.builtins);
            }
            if refs && valuable {
                t.sites += 1;
                t.site_builtins += i.builtins;
            } else if refs && i.any_op {
                t.trivial += 1;
            } else if !refs && i.builtins > 0 {
                t.const_only_builtin_sites += 1;
            }
            return;
        }
        if let ExprKind::Lambda {
            body: LambdaBody::Block(b),
            ..
        } = &e.kind
        {
            self.sites_stmts(b, sc, t);
        }
        for c in expr_children(e) {
            self.sites_expr(c, sc, t);
        }
    }

    fn sites_stmts(&self, ss: &[Stmt], sc: &Scope, t: &mut Tally) {
        for s in ss {
            let (mut es, mut subs) = (Vec::new(), Vec::new());
            stmt_exprs(s, &mut es, &mut subs);
            for e in es {
                self.sites_expr(e, sc, t);
            }
            for sub in subs {
                self.sites_stmts(std::slice::from_ref(sub), sc, t);
            }
        }
    }

    fn count_builtin_calls(&self, body: &[Stmt], locals: &BTreeSet<String>) -> usize {
        let mut n = 0;
        visit_stmts(body, &mut |e| {
            if let ExprKind::Call { callee, .. } = &e.kind {
                if let ExprKind::Ident(name) = &callee.kind {
                    if self.builtins.contains(name.as_str())
                        && !self.funcs.contains_key(name)
                        && !locals.contains(name)
                    {
                        n += 1;
                    }
                }
            }
        });
        n
    }
}

#[derive(Default)]
struct Rec {
    file: String,
    err: Option<String>,
    has_render: bool,
    direct: Tally,
    total_builtins: usize,
    strict: Tally,
    loose: Tally,
}

fn analyze(file: &str, src: &str, verbose: bool) -> Rec {
    let mut rec = Rec {
        file: file.to_string(),
        ..Default::default()
    };
    let prog = match parse_program(src) {
        Ok(p) => p,
        Err(d) => {
            rec.err = Some(format!("{d:?}"));
            return rec;
        }
    };

    // top level: functions, globals
    let mut funcs = BTreeMap::new();
    let mut globals = BTreeSet::new();
    let mut top_level: Vec<Stmt> = Vec::new();
    for s in &prog {
        match &s.kind {
            StmtKind::Func {
                name, params, body, ..
            } => {
                funcs.insert(
                    name.clone(),
                    Func {
                        params: params.as_slice(),
                        body: body.as_slice(),
                    },
                );
            }
            StmtKind::Var { decls, .. } => {
                globals.extend(decls.iter().map(|d| d.name.clone()));
                top_level.push(s.clone());
            }
            _ => top_level.push(s.clone()),
        }
    }
    globals.extend(written_names(&top_level));

    // anything written anywhere at all (for frozen predefined constants)
    let mut written_anywhere = written_names(&top_level);
    for f in funcs.values() {
        written_anywhere.extend(written_names(f.body));
    }
    let consts: BTreeSet<String> = PREDEF_CONSTS
        .iter()
        .filter(|c| !written_anywhere.contains(**c))
        .map(|c| c.to_string())
        .collect();
    globals.insert("pixelCount".to_string());
    for c in PREDEF_CONSTS {
        if !consts.contains(*c) {
            globals.insert(c.to_string());
        }
    }

    // array-ish names, whole program
    let mut arrayish = BTreeSet::new();
    let mut mark = |e: &Expr| match &e.kind {
        ExprKind::Index { obj, .. } | ExprKind::Member { obj, .. } => {
            if let ExprKind::Ident(n) = &obj.kind {
                arrayish.insert(n.clone());
            }
        }
        ExprKind::Assign { target, value, .. } if is_array_value(value) => {
            if let ExprKind::Ident(n) = &target.kind {
                arrayish.insert(n.clone());
            }
        }
        _ => {}
    };
    visit_stmts(&prog, &mut mark);
    for s in &prog {
        if let StmtKind::Var { decls, .. } = &s.kind {
            for d in decls {
                if d.init.as_ref().is_some_and(is_array_value) {
                    arrayish.insert(d.name.clone());
                }
            }
        }
    }

    // call graph reachability from render*
    let entries: Vec<&str> = ENTRIES
        .iter()
        .copied()
        .filter(|e| funcs.contains_key(*e))
        .collect();
    rec.has_render = !entries.is_empty();
    if !rec.has_render {
        return rec;
    }
    let mut reach: BTreeSet<String> = BTreeSet::new();
    let mut work: Vec<String> = entries.iter().map(|s| s.to_string()).collect();
    while let Some(fname) = work.pop() {
        if !reach.insert(fname.clone()) {
            continue;
        }
        let f = &funcs[&fname];
        visit_stmts(f.body, &mut |e| {
            if let ExprKind::Ident(n) = &e.kind {
                if funcs.contains_key(n) && !reach.contains(n) {
                    work.push(n.clone());
                }
            }
        });
    }
    let mut written_by_render = BTreeSet::new();
    for fname in &reach {
        let f = &funcs[fname];
        // locals for this test: params and declared vars only (lambda
        // params NOT excluded — a write there counts as a global write)
        let mut own: BTreeSet<String> = f.params.iter().cloned().collect();
        for s in f.body {
            collect_vars(s, &mut own);
        }
        for w in written_names(f.body) {
            if !own.contains(&w) {
                written_by_render.insert(w);
            }
        }
    }
    let inv_globals: BTreeSet<String> = globals
        .iter()
        .filter(|g| !arrayish.contains(*g) && !written_by_render.contains(*g))
        .filter(|g| !funcs.contains_key(*g))
        .cloned()
        .collect();

    let builtins: BTreeSet<&'static str> = BUILTINS
        .iter()
        .filter(|d| matches!(d.kind, BKind::Impl(_)))
        .map(|d| d.name)
        .collect();
    let p = Prog {
        funcs,
        inv_globals,
        consts,
        builtins,
        src,
        verbose,
    };
    if verbose {
        println!("== {file}");
    }

    let none = BTreeSet::new();
    for en in &entries {
        let f = &p.funcs[*en];
        let sc = Scope {
            locals: locals_of(f.params, f.body),
            inv_params: &none,
            tag: en,
        };
        p.sites_stmts(f.body, &sc, &mut rec.direct);
        rec.total_builtins += p.count_builtin_calls(f.body, &sc.locals);

        // inlining estimate: direct call sites to user functions
        let mut calls: Vec<(&String, &Vec<Expr>)> = Vec::new();
        visit_stmts(f.body, &mut |e| {
            if let ExprKind::Call { callee, args } = &e.kind {
                if let ExprKind::Ident(n) = &callee.kind {
                    // a call to another render* entry (`render` delegating
                    // to `render3D(i, x, 0.5, 0.5)`) is skipped: that body
                    // is already counted directly as an entry.
                    if p.funcs.contains_key(n)
                        && !sc.locals.contains(n)
                        && !ENTRIES.contains(&n.as_str())
                    {
                        calls.push((n, args));
                    }
                }
            }
        });
        for (callee, args) in calls {
            let g = &p.funcs[callee];
            let g_written = written_names(g.body);
            let mut strict = BTreeSet::new();
            let mut loose = BTreeSet::new();
            for (i, prm) in g.params.iter().enumerate() {
                if g_written.contains(prm) {
                    continue;
                }
                match args.get(i).map(|a| p.inv(a, &sc)) {
                    None => {
                        strict.insert(prm.clone());
                        loose.insert(prm.clone());
                    }
                    Some(Some(info)) => {
                        if info.globals == 0 {
                            strict.insert(prm.clone());
                        }
                        loose.insert(prm.clone());
                    }
                    Some(None) => {}
                }
            }
            let locals = locals_of(g.params, g.body);
            let s1 = Scope {
                locals: locals.clone(),
                inv_params: &strict,
                tag: "inline-strict",
            };
            p.sites_stmts(g.body, &s1, &mut rec.strict);
            let s2 = Scope {
                locals,
                inv_params: &loose,
                tag: "inline-loose",
            };
            p.sites_stmts(g.body, &s2, &mut rec.loose);
        }
    }
    rec
}

fn collect_vars(s: &Stmt, out: &mut BTreeSet<String>) {
    if let StmtKind::Var { decls, .. } = &s.kind {
        out.extend(decls.iter().map(|d| d.name.clone()));
    }
    let (mut es, mut subs) = (Vec::new(), Vec::new());
    stmt_exprs(s, &mut es, &mut subs);
    for sub in subs {
        collect_vars(sub, out);
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.first().is_some_and(|a| a == "--sites");
    if verbose {
        args.remove(0);
    }
    let mut files: Vec<String> = Vec::new();
    for a in &args {
        let path = std::path::Path::new(a);
        if path.is_dir() {
            let mut v: Vec<String> = std::fs::read_dir(path)
                .expect("read_dir")
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "js"))
                .map(|p| p.display().to_string())
                .collect();
            v.sort();
            files.extend(v);
        } else {
            files.push(a.clone());
        }
    }
    let recs: Vec<Rec> = files
        .iter()
        .map(|f| analyze(f, &std::fs::read_to_string(f).unwrap_or_default(), verbose))
        .collect();

    println!(
        "{:<44} {:>5} {:>6} {:>6} {:>5} | {:>9} {:>9}",
        "file", "sites", "hoistB", "totalB", "triv", "inlS s/B", "inlL s/B"
    );
    for r in &recs {
        let name = std::path::Path::new(&r.file)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Some(e) = &r.err {
            println!("{name:<44} PARSE FAIL: {e}");
            continue;
        }
        if !r.has_render {
            println!("{name:<44} (no render/render2D/render3D)");
            continue;
        }
        println!(
            "{:<44} {:>5} {:>6} {:>6} {:>5} | {:>4}/{:<4} {:>4}/{:<4}",
            name,
            r.direct.sites,
            r.direct.site_builtins,
            r.total_builtins,
            r.direct.trivial,
            r.strict.sites,
            r.strict.site_builtins,
            r.loose.sites,
            r.loose.site_builtins
        );
    }

    let failed: Vec<&Rec> = recs.iter().filter(|r| r.err.is_some()).collect();
    let ok: Vec<&Rec> = recs
        .iter()
        .filter(|r| r.err.is_none() && r.has_render)
        .collect();
    let norender = recs
        .iter()
        .filter(|r| r.err.is_none() && !r.has_render)
        .count();
    let sum = |f: &dyn Fn(&Rec) -> usize| ok.iter().map(|r| f(r)).sum::<usize>();
    let tot_b = sum(&|r| r.total_builtins);
    let hoist_b = sum(&|r| r.direct.site_builtins);
    println!("\n== summary ==");
    println!(
        "patterns: {} files, {} parse failures, {} without render*, {} analysed",
        recs.len(),
        failed.len(),
        norender,
        ok.len()
    );
    for r in &failed {
        println!("  PARSE FAIL {}", r.file);
    }
    println!(
        "patterns with >=1 hoistable site      {}",
        ok.iter().filter(|r| r.direct.sites > 0).count()
    );
    println!(
        "total hoistable sites                 {}",
        sum(&|r| r.direct.sites)
    );
    println!(
        "hoisted builtin calls / render* total {} / {} ({:.1} %)",
        hoist_b,
        tot_b,
        100.0 * hoist_b as f64 / tot_b.max(1) as f64
    );
    println!(
        "trivial invariant sites (no call, no * / % **) {} in {} patterns",
        sum(&|r| r.direct.trivial),
        ok.iter().filter(|r| r.direct.trivial > 0).count()
    );
    println!(
        "constant-only builtin sites (rule d excludes) {}",
        sum(&|r| r.direct.const_only_builtin_sites)
    );
    let extra = |f: &dyn Fn(&Rec) -> &Tally| {
        ok.iter()
            .filter(|r| r.direct.sites == 0 && f(r).sites > 0)
            .count()
    };
    let any = |f: &dyn Fn(&Rec) -> &Tally| ok.iter().filter(|r| f(r).sites > 0).count();
    println!("\n== with one level of inlining of user fns called from render* ==");
    println!(
        "STRICT (literal/constant args): {} patterns gain sites ({} ADDITIONAL, i.e. 0 direct); {} sites, {} builtin calls",
        any(&|r| &r.strict),
        extra(&|r| &r.strict),
        sum(&|r| r.strict.sites),
        sum(&|r| r.strict.site_builtins)
    );
    println!(
        "LOOSE  (any invariant args):    {} patterns gain sites ({} ADDITIONAL, i.e. 0 direct); {} sites, {} builtin calls",
        any(&|r| &r.loose),
        extra(&|r| &r.loose),
        sum(&|r| r.loose.sites),
        sum(&|r| r.loose.site_builtins)
    );

    let mut top: Vec<&&Rec> = ok.iter().filter(|r| r.direct.site_builtins > 0).collect();
    top.sort_by(|a, b| {
        b.direct
            .site_builtins
            .cmp(&a.direct.site_builtins)
            .then(b.direct.sites.cmp(&a.direct.sites))
    });
    println!("\n== top 15 by hoisted builtin calls ==");
    println!("{:<44} {:>5} {:>6} {:>6}", "file", "sites", "hoistB", "totalB");
    for r in top.iter().take(15) {
        let name = std::path::Path::new(&r.file)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        println!(
            "{:<44} {:>5} {:>6} {:>6}",
            name, r.direct.sites, r.direct.site_builtins, r.total_builtins
        );
    }
}
