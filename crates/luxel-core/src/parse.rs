//! Recursive-descent / Pratt parser for the Luxel pattern language.
//!
//! Semicolons are optional, JS-style: expressions consume greedily across
//! newlines whenever the grammar allows continuation, and a statement is
//! terminated by `;`, a newline, `}`, or EOF. The JS restricted productions
//! are honored: a newline after `return` means "no value", and a newline
//! before `++`/`--` prevents the postfix reading. This matches how the
//! Pixel Blaze corpus (written against a JS-based compiler) actually parses.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use crate::fixed::Fx;
use crate::lex::{lex, Tok, Token};

pub fn parse_program(src: &str) -> Result<Vec<Stmt>, Diagnostic> {
    let toks = lex(src)?;
    let mut p = Parser {
        src,
        toks,
        pos: 0,
        prev_span: Span::default(),
        depth: 0,
    };
    let mut stmts = Vec::new();
    while !p.at_eof() {
        stmts.push(p.stmt()?);
    }
    Ok(stmts)
}

/// Parse a single standalone EXPRESSION (a `//# require` directive body).
/// Trailing tokens are an error.
pub(crate) fn parse_expr_snippet(src: &str) -> Result<Expr, Diagnostic> {
    let toks = lex(src)?;
    let mut p = Parser {
        src,
        toks,
        pos: 0,
        prev_span: Span::default(),
        depth: 0,
    };
    let e = p.expr()?;
    if !p.at_eof() {
        return Err(Diagnostic::new(p.prev_span, "unexpected trailing tokens"));
    }
    Ok(e)
}

struct Parser<'s> {
    src: &'s str,
    toks: Vec<Token>,
    pos: usize,
    prev_span: Span,
    /// Current recursion depth (statements + expressions). Bounded so a
    /// pathologically nested source becomes a compile ERROR — on the
    /// firmware the parser shares a ~30 KB task stack with everything
    /// (incl. WiFi NMI frames), and unbounded recursion overflowed it in
    /// the field (stack-guard panic while soaking the corpus).
    depth: u32,
}

/// Deepest allowed statement/expression nesting. The heaviest community
/// pattern measures well under half of this; Xtensa frames for
/// stmt/expr levels are ~200-400 B, so 60 levels stays within a few KB.
const MAX_DEPTH: u32 = 60;

#[cfg(feature = "depth-probe")]
pub static PEAK: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

impl<'s> Parser<'s> {
    // ---- cursor helpers ----

    fn at_eof(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn peek(&self) -> Option<Tok> {
        self.toks.get(self.pos).map(|t| t.tok)
    }

    fn peek2(&self) -> Option<Tok> {
        self.toks.get(self.pos + 1).map(|t| t.tok)
    }

    /// Was there a newline between the previous token and the current one?
    fn nl_before(&self) -> bool {
        self.toks.get(self.pos).map(|t| t.nl_before).unwrap_or(true)
    }

    fn span_here(&self) -> Span {
        self.toks
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or(Span::new(self.src.len(), self.src.len()))
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos];
        self.pos += 1;
        self.prev_span = t.span;
        t
    }

    fn at(&self, tok: Tok) -> bool {
        self.peek() == Some(tok)
    }

    fn eat(&mut self, tok: Tok) -> bool {
        if self.at(tok) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: Tok, what: &str) -> Result<Token, Diagnostic> {
        if self.at(tok) {
            Ok(self.bump())
        } else {
            Err(self.err_here(format!("expected {what}")))
        }
    }

    fn err_here(&self, message: String) -> Diagnostic {
        let span = self.span_here();
        let found = self
            .toks
            .get(self.pos)
            .map(|t| {
                format!(
                    "`{}`",
                    &self.src[t.span.start as usize..t.span.end as usize]
                )
            })
            .unwrap_or_else(|| "end of input".to_string());
        Diagnostic::new(span, format!("{message}, found {found}"))
    }

    fn slice(&self, span: Span) -> &'s str {
        &self.src[span.start as usize..span.end as usize]
    }

    fn ident_name(&mut self, what: &str) -> Result<(String, Span), Diagnostic> {
        let t = self.expect(Tok::Ident, what)?;
        Ok((self.slice(t.span).to_string(), t.span))
    }

    /// Statement terminator: `;`, a newline before the next token, `}`, or EOF.
    fn terminate(&mut self) -> Result<(), Diagnostic> {
        match self.peek() {
            Some(Tok::Semi) => {
                self.bump();
                Ok(())
            }
            Some(Tok::RBrace) | None => Ok(()),
            Some(_) if self.nl_before() => Ok(()),
            Some(_) => Err(self.err_here("expected `;` or newline after statement".into())),
        }
    }

    // ---- statements ----

    fn stmt(&mut self) -> Result<Stmt, Diagnostic> {
        self.depth += 1;
        #[cfg(feature = "depth-probe")]
        crate::parse::PEAK.fetch_max(self.depth, core::sync::atomic::Ordering::Relaxed);
        let r = self.stmt_inner();
        self.depth -= 1;
        r
    }

    fn stmt_inner(&mut self) -> Result<Stmt, Diagnostic> {
        if self.depth > MAX_DEPTH {
            return Err(self.err_here("nesting too deep".into()));
        }
        let start = self.span_here();
        match self.peek() {
            None => Err(self.err_here("expected statement".into())),
            Some(Tok::Semi) => {
                self.bump();
                Ok(Stmt {
                    kind: StmtKind::Empty,
                    span: start,
                })
            }
            Some(Tok::LBrace) => self.block_stmt(),
            Some(Tok::Var | Tok::Let | Tok::Const) => {
                let s = self.var_stmt(false, start)?;
                Ok(s)
            }
            Some(Tok::Export) => {
                self.bump();
                match self.peek() {
                    Some(Tok::Var | Tok::Let | Tok::Const) => self.var_stmt(true, start),
                    Some(Tok::Function) => self.func_stmt(true, start),
                    _ => Err(self.err_here(
                        "expected `var`, `let`, `const`, or `function` after `export`".into(),
                    )),
                }
            }
            Some(Tok::Function) => self.func_stmt(false, start),
            Some(Tok::If) => self.if_stmt(),
            Some(Tok::While) => self.while_stmt(),
            Some(Tok::For) => self.for_stmt(),
            Some(Tok::Return) => {
                self.bump();
                let value = if self.return_value_follows() {
                    Some(self.expr()?)
                } else {
                    None
                };
                self.terminate()?;
                Ok(Stmt {
                    kind: StmtKind::Return(value),
                    span: start.to(self.prev_span),
                })
            }
            Some(Tok::Break) => {
                self.bump();
                self.terminate()?;
                Ok(Stmt {
                    kind: StmtKind::Break,
                    span: start,
                })
            }
            Some(Tok::Continue) => {
                self.bump();
                self.terminate()?;
                Ok(Stmt {
                    kind: StmtKind::Continue,
                    span: start,
                })
            }
            Some(_) => {
                let e = self.expr()?;
                self.terminate()?;
                Ok(Stmt {
                    span: e.span,
                    kind: StmtKind::Expr(e),
                })
            }
        }
    }

    /// Restricted production: `return` followed by a newline returns nothing.
    fn return_value_follows(&self) -> bool {
        if self.nl_before() {
            return false;
        }
        !matches!(self.peek(), None | Some(Tok::Semi) | Some(Tok::RBrace))
    }

    fn block_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.span_here();
        let body = self.block_body()?;
        Ok(Stmt {
            kind: StmtKind::Block(body),
            span: start.to(self.prev_span),
        })
    }

    fn block_body(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
        self.expect(Tok::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        while !self.at(Tok::RBrace) {
            if self.at_eof() {
                return Err(self.err_here("expected `}`".into()));
            }
            stmts.push(self.stmt()?);
        }
        self.bump(); // `}`
        Ok(stmts)
    }

    fn var_stmt(&mut self, export: bool, start: Span) -> Result<Stmt, Diagnostic> {
        let (kind, decls) = self.var_decls()?;
        self.terminate()?;
        Ok(Stmt {
            kind: StmtKind::Var {
                export,
                kind,
                decls,
            },
            span: start.to(self.prev_span),
        })
    }

    /// `var/let/const a = 1, b` — shared by var statements and for-loop
    /// initializers. `const` requires an initializer.
    fn var_decls(&mut self) -> Result<(DeclKind, Vec<VarDecl>), Diagnostic> {
        let kind = match self.peek() {
            Some(Tok::Let) => DeclKind::Let,
            Some(Tok::Const) => DeclKind::Const,
            _ => DeclKind::Var,
        };
        // any of var/let/const opens the declaration
        match self.peek() {
            Some(Tok::Var | Tok::Let | Tok::Const) => self.bump(),
            _ => return Err(self.err_here("expected `var`, `let`, or `const`".into())),
        };
        let mut decls = Vec::new();
        loop {
            let (name, name_span) = self.ident_name("variable name")?;
            let init = if self.eat(Tok::Assign) {
                Some(self.assign_expr()?)
            } else {
                if kind == DeclKind::Const {
                    return Err(Diagnostic::new(
                        name_span,
                        format!("`const {name}` must be initialized"),
                    ));
                }
                None
            };
            decls.push(VarDecl {
                span: name_span.to(self.prev_span),
                name,
                init,
            });
            if !self.eat(Tok::Comma) {
                break;
            }
        }
        Ok((kind, decls))
    }

    fn func_stmt(&mut self, export: bool, start: Span) -> Result<Stmt, Diagnostic> {
        self.expect(Tok::Function, "`function`")?;
        let (name, _) = self.ident_name("function name")?;
        self.expect(Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        while !self.at(Tok::RParen) {
            let (p, _) = self.ident_name("parameter name")?;
            params.push(p);
            if !self.eat(Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        let body = self.block_body()?;
        Ok(Stmt {
            kind: StmtKind::Func {
                export,
                name,
                params,
                body,
            },
            span: start.to(self.prev_span),
        })
    }

    fn if_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.span_here();
        self.bump(); // `if`
        self.expect(Tok::LParen, "`(` after `if`")?;
        let cond = self.expr()?;
        self.expect(Tok::RParen, "`)`")?;
        let then = Box::new(self.stmt()?);
        let els = if self.eat(Tok::Else) {
            Some(Box::new(self.stmt()?))
        } else {
            None
        };
        Ok(Stmt {
            kind: StmtKind::If { cond, then, els },
            span: start.to(self.prev_span),
        })
    }

    fn while_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.span_here();
        self.bump(); // `while`
        self.expect(Tok::LParen, "`(` after `while`")?;
        let cond = self.expr()?;
        self.expect(Tok::RParen, "`)`")?;
        let body = Box::new(self.stmt()?);
        Ok(Stmt {
            kind: StmtKind::While { cond, body },
            span: start.to(self.prev_span),
        })
    }

    fn for_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.span_here();
        self.bump(); // `for`
        self.expect(Tok::LParen, "`(` after `for`")?;

        let init = if self.eat(Tok::Semi) {
            None
        } else if self.at(Tok::Var) || self.at(Tok::Let) || self.at(Tok::Const) {
            let var_start = self.span_here();
            let (kind, decls) = self.var_decls()?;
            self.expect(Tok::Semi, "`;` after for-loop initializer")?;
            Some(Box::new(Stmt {
                kind: StmtKind::Var {
                    export: false,
                    kind,
                    decls,
                },
                span: var_start.to(self.prev_span),
            }))
        } else {
            let e = self.expr()?;
            self.expect(Tok::Semi, "`;` after for-loop initializer")?;
            Some(Box::new(Stmt {
                span: e.span,
                kind: StmtKind::Expr(e),
            }))
        };

        let cond = if self.at(Tok::Semi) {
            None
        } else {
            Some(self.expr()?)
        };
        self.expect(Tok::Semi, "`;` after for-loop condition")?;

        let update = if self.at(Tok::RParen) {
            None
        } else {
            Some(self.expr()?)
        };
        self.expect(Tok::RParen, "`)`")?;

        let body = Box::new(self.stmt()?);
        Ok(Stmt {
            kind: StmtKind::For {
                init,
                cond,
                update,
                body,
            },
            span: start.to(self.prev_span),
        })
    }

    // ---- expressions ----

    fn expr(&mut self) -> Result<Expr, Diagnostic> {
        self.assign_expr()
    }

    fn assign_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.depth += 1;
        #[cfg(feature = "depth-probe")]
        crate::parse::PEAK.fetch_max(self.depth, core::sync::atomic::Ordering::Relaxed);
        if self.depth > MAX_DEPTH {
            self.depth -= 1;
            return Err(self.err_here("expression nesting too deep".into()));
        }
        let r = self.assign_expr_inner();
        self.depth -= 1;
        r
    }

    fn assign_expr_inner(&mut self) -> Result<Expr, Diagnostic> {
        let lhs = self.ternary_expr()?;
        let op = match self.peek() {
            Some(Tok::Assign) => Some(None),
            Some(Tok::PlusAssign) => Some(Some(BinOp::Add)),
            Some(Tok::MinusAssign) => Some(Some(BinOp::Sub)),
            Some(Tok::StarAssign) => Some(Some(BinOp::Mul)),
            Some(Tok::SlashAssign) => Some(Some(BinOp::Div)),
            Some(Tok::PercentAssign) => Some(Some(BinOp::Rem)),
            Some(Tok::ShlAssign) => Some(Some(BinOp::Shl)),
            Some(Tok::ShrAssign) => Some(Some(BinOp::Shr)),
            Some(Tok::AmpAssign) => Some(Some(BinOp::BitAnd)),
            Some(Tok::PipeAssign) => Some(Some(BinOp::BitOr)),
            Some(Tok::CaretAssign) => Some(Some(BinOp::BitXor)),
            _ => None,
        };
        let Some(op) = op else { return Ok(lhs) };
        self.check_target(&lhs, "assignment")?;
        self.bump();
        let value = self.assign_expr()?; // right-associative
        let span = lhs.span.to(value.span);
        Ok(Expr {
            kind: ExprKind::Assign {
                op,
                target: Box::new(lhs),
                value: Box::new(value),
            },
            span,
        })
    }

    /// Assignable places: variables and array elements. There are no
    /// assignable properties in the language (`a.length = …` is an error).
    fn check_target(&self, e: &Expr, what: &str) -> Result<(), Diagnostic> {
        match e.kind {
            ExprKind::Ident(_) | ExprKind::Index { .. } => Ok(()),
            _ => Err(Diagnostic::new(e.span, format!("invalid {what} target"))),
        }
    }

    fn ternary_expr(&mut self) -> Result<Expr, Diagnostic> {
        let cond = self.binary_expr(0)?;
        if !self.eat(Tok::Question) {
            return Ok(cond);
        }
        let then = self.assign_expr()?;
        self.expect(Tok::Colon, "`:` in conditional expression")?;
        let els = self.assign_expr()?;
        let span = cond.span.to(els.span);
        Ok(Expr {
            kind: ExprKind::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
            },
            span,
        })
    }

    fn binary_expr(&mut self, min_prec: u8) -> Result<Expr, Diagnostic> {
        let mut lhs = self.unary_expr()?;
        while let Some(tok) = self.peek() {
            let Some((op, prec)) = bin_op(tok) else { break };
            if prec < min_prec {
                break;
            }
            self.bump();
            // `**` is right-associative (2**3**2 = 512, like JS); everything
            // else left-assoc. Unlike JS we accept a unary lhs: -x ** 2
            // parses as (-x) ** 2 (the unary parser binds first).
            let next_min = if op == BinOp::Pow { prec } else { prec + 1 };
            let rhs = self.binary_expr(next_min)?;
            let span = lhs.span.to(rhs.span);
            lhs = Expr {
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }
        Ok(lhs)
    }

    fn unary_expr(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.span_here();
        let op = match self.peek() {
            Some(Tok::Minus) => Some(UnOp::Neg),
            Some(Tok::Plus) => Some(UnOp::Pos),
            Some(Tok::Bang) => Some(UnOp::Not),
            Some(Tok::Tilde) => Some(UnOp::BitNot),
            Some(Tok::PlusPlus) | Some(Tok::MinusMinus) => {
                let inc = self.at(Tok::PlusPlus);
                self.bump();
                let target = self.unary_expr()?;
                self.check_target(&target, "increment")?;
                let span = start.to(target.span);
                return Ok(Expr {
                    kind: ExprKind::IncDec {
                        inc,
                        prefix: true,
                        target: Box::new(target),
                    },
                    span,
                });
            }
            _ => None,
        };
        let Some(op) = op else {
            return self.postfix_expr();
        };
        self.bump();
        let expr = self.unary_expr()?;
        let span = start.to(expr.span);
        Ok(Expr {
            kind: ExprKind::Unary {
                op,
                expr: Box::new(expr),
            },
            span,
        })
    }

    fn postfix_expr(&mut self) -> Result<Expr, Diagnostic> {
        let mut e = self.primary_expr()?;
        loop {
            match self.peek() {
                Some(Tok::LParen) => {
                    self.bump();
                    let args = self.call_args()?;
                    let span = e.span.to(self.prev_span);
                    e = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(e),
                            args,
                        },
                        span,
                    };
                }
                Some(Tok::LBracket) => {
                    self.bump();
                    let index = self.expr()?;
                    self.expect(Tok::RBracket, "`]`")?;
                    let span = e.span.to(self.prev_span);
                    e = Expr {
                        kind: ExprKind::Index {
                            obj: Box::new(e),
                            index: Box::new(index),
                        },
                        span,
                    };
                }
                Some(Tok::Dot) => {
                    self.bump();
                    let (name, _) = self.ident_name("property name after `.`")?;
                    let span = e.span.to(self.prev_span);
                    e = Expr {
                        kind: ExprKind::Member {
                            obj: Box::new(e),
                            name,
                        },
                        span,
                    };
                }
                // Restricted production: a newline prevents the postfix
                // reading, so `a \n ++b` is two statements like in JS.
                Some(Tok::PlusPlus) | Some(Tok::MinusMinus) if !self.nl_before() => {
                    let inc = self.at(Tok::PlusPlus);
                    self.check_target(&e, "increment")?;
                    self.bump();
                    let span = e.span.to(self.prev_span);
                    e = Expr {
                        kind: ExprKind::IncDec {
                            inc,
                            prefix: false,
                            target: Box::new(e),
                        },
                        span,
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn call_args(&mut self) -> Result<Vec<Expr>, Diagnostic> {
        let mut args = Vec::new();
        while !self.at(Tok::RParen) {
            args.push(self.assign_expr()?);
            if !self.eat(Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        Ok(args)
    }

    fn primary_expr(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.span_here();
        match self.peek() {
            Some(Tok::Number) => {
                let t = self.bump();
                let text = self.slice(t.span);
                let value = Fx::parse_literal(text).ok_or_else(|| {
                    Diagnostic::new(t.span, format!("invalid number literal `{text}`"))
                })?;
                Ok(Expr {
                    kind: ExprKind::Num(value),
                    span: t.span,
                })
            }
            Some(Tok::True) => {
                let t = self.bump();
                Ok(Expr {
                    kind: ExprKind::Num(Fx::ONE),
                    span: t.span,
                })
            }
            Some(Tok::False) => {
                let t = self.bump();
                Ok(Expr {
                    kind: ExprKind::Num(Fx::ZERO),
                    span: t.span,
                })
            }
            Some(Tok::Ident) => {
                // `x => body` single-parameter lambda
                if self.peek2() == Some(Tok::FatArrow) {
                    let (name, _) = self.ident_name("parameter name")?;
                    self.bump(); // `=>`
                    let body = self.lambda_body()?;
                    return Ok(Expr {
                        kind: ExprKind::Lambda {
                            params: alloc::vec![name],
                            body,
                        },
                        span: start.to(self.prev_span),
                    });
                }
                let (name, span) = self.ident_name("identifier")?;
                Ok(Expr {
                    kind: ExprKind::Ident(name),
                    span,
                })
            }
            Some(Tok::LParen) => {
                if self.arrow_ahead() {
                    self.bump(); // `(`
                    let mut params = Vec::new();
                    while !self.at(Tok::RParen) {
                        let (p, _) = self.ident_name("parameter name")?;
                        params.push(p);
                        if !self.eat(Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(Tok::RParen, "`)`")?;
                    self.expect(Tok::FatArrow, "`=>`")?;
                    let body = self.lambda_body()?;
                    Ok(Expr {
                        kind: ExprKind::Lambda { params, body },
                        span: start.to(self.prev_span),
                    })
                } else {
                    self.bump();
                    let mut e = self.expr()?;
                    self.expect(Tok::RParen, "`)`")?;
                    e.span = start.to(self.prev_span);
                    Ok(e)
                }
            }
            Some(Tok::LBracket) => {
                self.bump();
                let mut elems = Vec::new();
                while !self.at(Tok::RBracket) {
                    elems.push(self.assign_expr()?);
                    if !self.eat(Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RBracket, "`]`")?;
                Ok(Expr {
                    kind: ExprKind::ArrayLit(elems),
                    span: start.to(self.prev_span),
                })
            }
            // function expression: `f = function (a) { … }` (a name after
            // `function` is allowed and ignored — no closures, no self-ref)
            Some(Tok::Function) => {
                self.bump();
                if self.at(Tok::Ident) {
                    self.bump();
                }
                self.expect(Tok::LParen, "`(`")?;
                let mut params = Vec::new();
                while !self.at(Tok::RParen) {
                    let (p, _) = self.ident_name("parameter name")?;
                    params.push(p);
                    if !self.eat(Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RParen, "`)`")?;
                let body = self.block_body()?;
                Ok(Expr {
                    kind: ExprKind::Lambda {
                        params,
                        body: LambdaBody::Block(body),
                    },
                    span: start.to(self.prev_span),
                })
            }
            _ => Err(self.err_here("expected an expression".into())),
        }
    }

    /// At a `(`: does the matching `)` have a `=>` right after it? Decides
    /// lambda-vs-parenthesized-expression without backtracking.
    fn arrow_ahead(&self) -> bool {
        let mut depth = 0usize;
        let mut i = self.pos;
        while let Some(t) = self.toks.get(i) {
            match t.tok {
                Tok::LParen => depth += 1,
                Tok::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return self.toks.get(i + 1).map(|t| t.tok) == Some(Tok::FatArrow);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn lambda_body(&mut self) -> Result<LambdaBody, Diagnostic> {
        if self.at(Tok::LBrace) {
            Ok(LambdaBody::Block(self.block_body()?))
        } else {
            Ok(LambdaBody::Expr(Box::new(self.assign_expr()?)))
        }
    }
}

fn bin_op(tok: Tok) -> Option<(BinOp, u8)> {
    Some(match tok {
        Tok::OrOr => (BinOp::Or, 1),
        Tok::AndAnd => (BinOp::And, 2),
        Tok::Pipe => (BinOp::BitOr, 3),
        Tok::Caret => (BinOp::BitXor, 4),
        Tok::Amp => (BinOp::BitAnd, 5),
        Tok::EqEq => (BinOp::Eq, 6),
        Tok::NotEq => (BinOp::Ne, 6),
        Tok::Lt => (BinOp::Lt, 7),
        Tok::Le => (BinOp::Le, 7),
        Tok::Gt => (BinOp::Gt, 7),
        Tok::Ge => (BinOp::Ge, 7),
        Tok::Shl => (BinOp::Shl, 8),
        Tok::Shr => (BinOp::Shr, 8),
        Tok::Plus => (BinOp::Add, 9),
        Tok::Minus => (BinOp::Sub, 9),
        Tok::Star => (BinOp::Mul, 10),
        Tok::Slash => (BinOp::Div, 10),
        Tok::Percent => (BinOp::Rem, 10),
        Tok::StarStar => (BinOp::Pow, 11),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Vec<Stmt> {
        match parse_program(src) {
            Ok(s) => s,
            Err(d) => panic!("parse error at {:?}: {}", d.span, d.message),
        }
    }

    #[test]
    fn default_rainbow() {
        let stmts =
            parse("export function render(index) {\n  hsv(time(.1) + index/pixelCount, 1, 1)\n}");
        assert_eq!(stmts.len(), 1);
        let StmtKind::Func {
            export,
            name,
            params,
            body,
        } = &stmts[0].kind
        else {
            panic!("expected function")
        };
        assert!(*export);
        assert_eq!(name, "render");
        assert_eq!(params, &["index"]);
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn two_function_pattern() {
        let stmts = parse(
            "export function beforeRender(delta) { t1 = time(.1) }\n\
             export function render(index, x) { hsv(t1 + x, 1, 1) }",
        );
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn blinkfade_shape() {
        // frame-buffer idiom: top-level allocation, decay loop, compound assign
        parse(
            "values = array(pixelCount)\n\
             fade = 0.02\n\
             export function beforeRender(delta) {\n\
               for (var i = 0; i < pixelCount; i++) {\n\
                 values[i] -= fade * delta * .1\n\
                 if (values[i] < 0) values[i] = random(1)\n\
               }\n\
             }\n\
             export function render(index) {\n\
               v = values[index]\n\
               hsv(h, 1, v * v)\n\
             }",
        );
    }

    #[test]
    fn asi_basic() {
        assert_eq!(parse("x = 1\ny = 2").len(), 2);
        assert!(parse_program("x = 1 y = 2").is_err());
        // greedy continuation across newlines, like JS
        assert_eq!(parse("a = b\n  + c").len(), 1);
        assert_eq!(parse("a = b +\n  c").len(), 1);
    }

    #[test]
    fn asi_return_restriction() {
        let stmts = parse("function f() { return\n5 }");
        let StmtKind::Func { body, .. } = &stmts[0].kind else {
            panic!()
        };
        assert_eq!(body.len(), 2);
        assert!(matches!(body[0].kind, StmtKind::Return(None)));
        // and without the newline the value attaches
        let stmts = parse("function f() { return 5 }");
        let StmtKind::Func { body, .. } = &stmts[0].kind else {
            panic!()
        };
        assert!(matches!(body[0].kind, StmtKind::Return(Some(_))));
    }

    #[test]
    fn asi_postfix_restriction() {
        let stmts = parse("a\n++b");
        assert_eq!(stmts.len(), 2);
        assert!(matches!(
            &stmts[1].kind,
            StmtKind::Expr(Expr {
                kind: ExprKind::IncDec { prefix: true, .. },
                ..
            })
        ));
        let stmts = parse("a++\nb");
        assert!(matches!(
            &stmts[0].kind,
            StmtKind::Expr(Expr {
                kind: ExprKind::IncDec { prefix: false, .. },
                ..
            })
        ));
    }

    #[test]
    fn lambdas() {
        parse("f = (a) => a * 2");
        parse("f = a => a * 2");
        parse("f = () => { g(1) }");
        parse("modes[0] = (index) => { h = 0.5; v = index % 2 }");
        // paren-expr is not misread as a lambda
        let stmts = parse("x = (a + b) * 2");
        assert!(matches!(
            &stmts[0].kind,
            StmtKind::Expr(Expr {
                kind: ExprKind::Assign { .. },
                ..
            })
        ));
    }

    #[test]
    fn dispatch_table_idiom() {
        parse(
            "modes = array(2)\n\
             modes[0] = () => 1\n\
             modes[1] = (index) => index % 2\n\
             result = modes[currentMode]()",
        );
    }

    #[test]
    fn arrays_and_members() {
        parse("a = [1, 2, 3]");
        parse("a = [[0, 1], [1, 0],]"); // nested + trailing comma
        parse("n = a.length");
        parse("a.mutate(v => v * 0.9)");
        parse("s = arraySum(a)");
    }

    #[test]
    fn controls_and_exports() {
        parse("export var speed = 0.5");
        parse("export var frequencyData"); // sensor binding, no initializer
        parse("var a = 1, b, c = 3");
        parse(
            "var mySetting = 0.5\n\
             export function sliderMy_Setting(v) { mySetting = v }",
        );
    }

    #[test]
    fn control_flow() {
        parse("if (a > 1) b = 2\nelse b = 3");
        parse("if (a) { b() } else if (c) { d() } else { e() }");
        parse("while (i < 10) i++");
        parse("for (;;) { break }");
        parse("for (i = 0; i < 10; i++) continue");
        parse("for (var i = 0, j = 1; i < j; i += 2) {}");
    }

    #[test]
    fn precedence() {
        // + binds tighter than <<
        let stmts = parse("x = 1 << 2 + 3");
        let StmtKind::Expr(Expr {
            kind: ExprKind::Assign { value, .. },
            ..
        }) = &stmts[0].kind
        else {
            panic!()
        };
        let ExprKind::Binary { op, rhs, .. } = &value.kind else {
            panic!()
        };
        assert_eq!(*op, BinOp::Shl);
        assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Add, .. }));
        // || is looser than &&
        let stmts = parse("x = a || b && c");
        let StmtKind::Expr(Expr {
            kind: ExprKind::Assign { value, .. },
            ..
        }) = &stmts[0].kind
        else {
            panic!()
        };
        assert!(matches!(value.kind, ExprKind::Binary { op: BinOp::Or, .. }));
        // ternary + assignment right-assoc
        parse("x = a ? b : c ? d : e");
        parse("a = b = c");
    }

    #[test]
    fn invalid_targets() {
        assert!(parse_program("5 = x").is_err());
        assert!(parse_program("a.length = 3").is_err());
        assert!(parse_program("f() = 3").is_err());
        assert!(parse_program("++5").is_err());
        // array elements ARE assignable
        parse("a[0] = 1");
        parse("a[i]++");
    }

    #[test]
    fn number_literals_in_context() {
        parse("t = time(.015)");
        parse("w = 0xFDB9");
        parse("mask = 0b1010");
        parse("x = 5.");
    }

    #[test]
    fn comments_and_blank_lines() {
        parse("// Blink fade\n\n/* multi\nline */\nvalues = array(pixelCount) // trailing\n");
    }
}
