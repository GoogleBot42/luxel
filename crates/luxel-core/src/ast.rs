//! AST for the Luxel pattern language.
//!
//! The language has no objects, closures, or runtime strings; identifiers are
//! kept as owned strings here and interned during compilation to bytecode.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::diag::Span;
use crate::fixed::Fx;

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Num(Fx),
    Ident(String),
    ArrayLit(Vec<Expr>),
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `target = value` or compound `target op= value`.
    Assign {
        op: Option<BinOp>,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    /// `++x`, `x--`, … — desugared to assignment during compilation, kept
    /// distinct here because postfix has a different result value.
    IncDec {
        inc: bool,
        prefix: bool,
        target: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
    },
    /// `.length` and array methods (`a.mutate(fn)`); resolved during
    /// compilation to builtins — there are no user-defined properties.
    Member {
        obj: Box<Expr>,
        name: String,
    },
    Lambda {
        params: Vec<String>,
        body: LambdaBody,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LambdaBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    /// Unary `+` — numeric identity in a one-type language, kept for fidelity.
    Pos,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    /// Short-circuiting; yields an operand value like JS (`0 || 42` → 42).
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `var a = 1, b` / `export var x`.
    Var {
        export: bool,
        decls: Vec<VarDecl>,
    },
    /// `function f(a) {…}` / `export function render(index) {…}`.
    Func {
        export: bool,
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },
    Expr(Expr),
    If {
        cond: Expr,
        then: Box<Stmt>,
        els: Option<Box<Stmt>>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    Block(Vec<Stmt>),
    Return(Option<Expr>),
    Break,
    Continue,
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub name: String,
    pub init: Option<Expr>,
    pub span: Span,
}
