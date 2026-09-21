//! The pattern-language prelude (Gitea #626, docs/jit-design.md §4).
//!
//! `arrayForEach`, `arrayMutate`, `arrayMapTo`, `arrayReduce`, `arraySortBy`
//! and `mapPixels` used to be `Vm::call_builtin` arms that re-entered the
//! interpreter through `dispatch_direct`. They are now written in the
//! pattern language (`prelude.js`, bundled with `include_str!`), parsed by
//! this module and linked into a program **only when it uses one** —
//! transitively, so a helper that called another would pull it in too.
//!
//! Why: a builtin that calls back into a pattern function is the one shape
//! an on-device JIT cannot compile without a Rust → native trampoline
//! (docs/jit-design.md §4). As pattern functions they are ordinary code, and
//! `compile::Compiler` specialises a copy per static callback so the
//! callback keeps typed parameters instead of the `Dyn` a `CallValue` forces.
//!
//! Nothing here pollutes the user's namespace: a linked helper is a plain
//! named function (so a user function of the same name shadows it, exactly
//! as a user function shadowed the builtin), its `var`s are locals, and an
//! unused helper is never linked at all.

use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ast::*;
use crate::diag::Diagnostic;
use crate::parse::parse_program;

/// The prelude source, compiled from the same front end user patterns use.
pub const PRELUDE_SRC: &str = include_str!("prelude.js");

/// One prelude helper and where its callback parameter sits.
pub(crate) struct PreludeFn {
    pub name: &'static str,
    /// Parameter index holding the callback — the parameter the compiler
    /// drops when it specialises the helper for a static callback.
    pub callback_param: u8,
}

/// Every helper `prelude.js` defines. The order is the source order; a name
/// not listed here is not linkable, so adding a function to `prelude.js`
/// without listing it is a no-op rather than a silent new global.
pub(crate) static PRELUDE_FNS: &[PreludeFn] = &[
    PreludeFn { name: "arrayForEach", callback_param: 1 },
    PreludeFn { name: "arrayMutate", callback_param: 1 },
    PreludeFn { name: "arrayMapTo", callback_param: 2 },
    PreludeFn { name: "arrayReduce", callback_param: 1 },
    PreludeFn { name: "arraySortBy", callback_param: 1 },
    PreludeFn { name: "mapPixels", callback_param: 0 },
];

/// Is `name` a prelude helper, and where is its callback?
pub(crate) fn info(name: &str) -> Option<&'static PreludeFn> {
    PRELUDE_FNS.iter().find(|p| p.name == name)
}

/// The prelude helpers `stmts` may reach, as top-level `function` decls in
/// prelude source order, ready to be prepended to the user's program.
///
/// `shadowed` are the names the pattern defines itself — a user function
/// wins over a prelude helper, so those are not linked at all.
///
/// The "may reach" test is deliberately syntactic and over-approximate: any
/// occurrence of the identifier, plus any method form (`a.mutate(f)`) whose
/// global name is a helper. Over-approximating costs a few dozen unused
/// words in the blob; under-approximating would be an `unknown identifier`.
pub(crate) fn link(top: &[Stmt], shadowed: &BTreeSet<String>) -> Result<Vec<Stmt>, Diagnostic> {
    let mut used: BTreeSet<String> = BTreeSet::new();
    name_uses_stmts(top, &mut used);
    let mut want: BTreeSet<String> = BTreeSet::new();
    for p in PRELUDE_FNS {
        if shadowed.contains(p.name) {
            continue;
        }
        if used.contains(p.name) || method_form_used(p.name, &used) {
            want.insert(p.name.to_string());
        }
    }
    if want.is_empty() {
        return Ok(Vec::new());
    }
    // Parse only when something is wanted: every compile would otherwise
    // pay for a prelude the pattern never mentions.
    let parsed = parse_program(PRELUDE_SRC)?;
    // Transitive closure: a helper that names another helper pulls it in.
    loop {
        let mut grew = false;
        for s in &parsed {
            let StmtKind::Func { name, body, .. } = &s.kind else {
                continue;
            };
            if !want.contains(name) {
                continue;
            }
            let mut inner: BTreeSet<String> = BTreeSet::new();
            name_uses_stmts(body, &mut inner);
            for p in PRELUDE_FNS {
                if !shadowed.contains(p.name)
                    && inner.contains(p.name)
                    && want.insert(p.name.to_string())
                {
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    Ok(parsed
        .into_iter()
        .filter(|s| match &s.kind {
            StmtKind::Func { name, .. } => want.contains(name),
            _ => false,
        })
        .collect())
}

/// Does `used` hold a method name (`.mutate`) whose global form is `global`?
fn method_form_used(global: &str, used: &BTreeSet<String>) -> bool {
    used.iter()
        .any(|m| crate::vm::method_global(m) == Some(global))
}

/// True when `cb` appears in `body` ONLY as the callee of a direct call.
/// That is exactly the condition under which the compiler may drop the
/// parameter and bind the callback statically: if the helper also stored it,
/// passed it on, or compared it, the parameter has to survive.
pub(crate) fn callback_is_call_only(body: &[Stmt], cb: &str) -> bool {
    let mut total = 0usize;
    let mut as_callee = 0usize;
    count_uses_stmts(body, cb, &mut total, &mut as_callee);
    total == as_callee
}

// ---- AST walks ----

fn name_uses_stmts(stmts: &[Stmt], out: &mut BTreeSet<String>) {
    walk_stmts(stmts, &mut |e| match &e.kind {
        ExprKind::Ident(n) => {
            out.insert(n.clone());
        }
        ExprKind::Member { name, .. } => {
            out.insert(name.clone());
        }
        _ => {}
    });
}

fn count_uses_stmts(stmts: &[Stmt], cb: &str, total: &mut usize, as_callee: &mut usize) {
    walk_stmts(stmts, &mut |e| match &e.kind {
        ExprKind::Ident(n) if n == cb => *total += 1,
        ExprKind::Call { callee, .. } => {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == cb) {
                *as_callee += 1;
            }
        }
        _ => {}
    });
}

/// Visit every expression in `stmts`, outside-in, including the bodies of
/// nested functions and lambdas.
fn walk_stmts(stmts: &[Stmt], f: &mut impl FnMut(&Expr)) {
    for s in stmts {
        walk_stmt(s, f);
    }
}

fn walk_stmt(s: &Stmt, f: &mut impl FnMut(&Expr)) {
    match &s.kind {
        StmtKind::Var { decls, .. } => {
            for d in decls {
                if let Some(e) = &d.init {
                    walk_expr(e, f);
                }
            }
        }
        StmtKind::Func { body, .. } => walk_stmts(body, f),
        StmtKind::Expr(e) => walk_expr(e, f),
        StmtKind::If { cond, then, els } => {
            walk_expr(cond, f);
            walk_stmt(then, f);
            if let Some(e) = els {
                walk_stmt(e, f);
            }
        }
        StmtKind::While { cond, body } => {
            walk_expr(cond, f);
            walk_stmt(body, f);
        }
        StmtKind::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt(i, f);
            }
            if let Some(c) = cond {
                walk_expr(c, f);
            }
            if let Some(u) = update {
                walk_expr(u, f);
            }
            walk_stmt(body, f);
        }
        StmtKind::Switch { disc, cases } => {
            walk_expr(disc, f);
            for c in cases {
                if let Some(t) = &c.test {
                    walk_expr(t, f);
                }
                walk_stmts(&c.body, f);
            }
        }
        StmtKind::Block(b) => walk_stmts(b, f),
        StmtKind::Return(Some(e)) => walk_expr(e, f),
        StmtKind::Assert { cond, .. } => walk_expr(cond, f),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Empty => {}
    }
}

fn walk_expr(e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Num(_) | ExprKind::Ident(_) => {}
        ExprKind::ArrayLit(es) => {
            for x in es {
                walk_expr(x, f);
            }
        }
        ExprKind::Unary { expr, .. } => walk_expr(expr, f),
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, f);
            walk_expr(rhs, f);
        }
        ExprKind::Assign { target, value, .. } => {
            walk_expr(target, f);
            walk_expr(value, f);
        }
        ExprKind::IncDec { target, .. } => walk_expr(target, f),
        ExprKind::Ternary { cond, then, els } => {
            walk_expr(cond, f);
            walk_expr(then, f);
            walk_expr(els, f);
        }
        ExprKind::Call { callee, args } => {
            walk_expr(callee, f);
            for a in args {
                walk_expr(a, f);
            }
        }
        ExprKind::Index { obj, index } => {
            walk_expr(obj, f);
            walk_expr(index, f);
        }
        ExprKind::Member { obj, .. } => walk_expr(obj, f),
        ExprKind::Lambda { body, .. } => match body {
            LambdaBody::Expr(x) => walk_expr(x, f),
            LambdaBody::Block(b) => walk_stmts(b, f),
        },
    }
}

/// The prelude parses and every listed helper is present — a build-time
/// guarantee that `include_str!` and `PRELUDE_FNS` agree.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_helper_exists_with_its_callback_parameter() {
        let parsed = parse_program(PRELUDE_SRC).expect("the prelude must parse");
        for p in PRELUDE_FNS {
            let f = parsed
                .iter()
                .find_map(|s| match &s.kind {
                    StmtKind::Func { name, params, body, .. } if name == p.name => {
                        Some((params, body))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("prelude.js has no `{}`", p.name));
            let (params, body) = f;
            assert!(
                (p.callback_param as usize) < params.len(),
                "{}: callback_param {} out of range",
                p.name,
                p.callback_param
            );
            assert!(
                callback_is_call_only(body, &params[p.callback_param as usize]),
                "{}: the callback parameter escapes, so no call site can specialise",
                p.name
            );
        }
    }

    #[test]
    fn the_prelude_declares_nothing_but_the_listed_helpers() {
        let parsed = parse_program(PRELUDE_SRC).expect("the prelude must parse");
        for s in &parsed {
            match &s.kind {
                StmtKind::Func { name, export, .. } => {
                    assert!(!export, "{name}: a prelude helper must not be exported");
                    assert!(info(name).is_some(), "{name} is not in PRELUDE_FNS");
                }
                _ => panic!("the prelude may hold nothing but function declarations"),
            }
        }
    }
}
