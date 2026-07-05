//! Lexer for the Luxel pattern language, built on `logos`.
//!
//! Newlines are significant only for automatic semicolon insertion, so the
//! public `lex()` folds them into an `nl_before` flag on the following token
//! rather than emitting them — the parser then implements JS-style ASI
//! (greedy expression continuation, with the `return`/postfix restrictions).

use alloc::vec::Vec;
use logos::Logos;

use crate::diag::{Diagnostic, Span};

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
#[logos(skip r"[ \t\r]+")]
#[logos(skip r"//[^\n]*")]
pub enum Tok {
    #[token("\n")]
    Newline,

    /// `/* … */`, scanned by callback (regex-based comment skipping trips up
    /// the DFA). Payload: did the comment contain a newline? — per JS, a
    /// multiline comment acts as a line terminator for ASI. Filtered out by
    /// `lex()`, never reaches the parser. An unterminated comment swallows
    /// the rest of the file, permissively.
    #[token("/*", block_comment)]
    BlockComment(bool),

    // Keywords
    #[token("var")]
    Var,
    #[token("function")]
    Function,
    #[token("export")]
    Export,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("for")]
    For,
    #[token("while")]
    While,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("return")]
    Return,
    #[token("true")]
    True,
    #[token("false")]
    False,

    #[regex(r"[A-Za-z_$][A-Za-z0-9_$]*")]
    Ident,

    // `.015`-style literals are idiomatic (e.g. `time(.015)`), hence the
    // leading-dot alternative; plain `.` still lexes as Dot for members.
    #[regex(r"[0-9]+\.[0-9]*|\.[0-9]+|[0-9]+|0[xX][0-9a-fA-F]+|0[bB][01]+")]
    Number,

    // Punctuation
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(",")]
    Comma,
    #[token(";")]
    Semi,
    #[token(":")]
    Colon,
    #[token("?")]
    Question,
    #[token(".")]
    Dot,
    #[token("=>")]
    FatArrow,

    // Operators
    #[token("=")]
    Assign,
    #[token("+=")]
    PlusAssign,
    #[token("-=")]
    MinusAssign,
    #[token("*=")]
    StarAssign,
    #[token("/=")]
    SlashAssign,
    #[token("%=")]
    PercentAssign,
    #[token("<<=")]
    ShlAssign,
    #[token(">>=")]
    ShrAssign,
    #[token("&=")]
    AmpAssign,
    #[token("|=")]
    PipeAssign,
    #[token("^=")]
    CaretAssign,
    #[token("++")]
    PlusPlus,
    #[token("--")]
    MinusMinus,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("<<")]
    Shl,
    #[token(">>")]
    Shr,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("!")]
    Bang,
}

fn block_comment(lex: &mut logos::Lexer<Tok>) -> bool {
    let rest = lex.remainder();
    let (body, consumed) = match rest.find("*/") {
        Some(end) => (&rest[..end], end + 2),
        None => (rest, rest.len()),
    };
    let has_newline = body.contains('\n');
    lex.bump(consumed);
    has_newline
}

/// A lexed token with its source span and whether at least one newline
/// separated it from the previous token (drives ASI in the parser).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    pub nl_before: bool,
}

/// Lex the whole source. Newline tokens are folded into `nl_before` flags.
pub fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut out = Vec::new();
    let mut nl_pending = false;
    let mut lexer = Tok::lexer(src);
    while let Some(result) = lexer.next() {
        let span = Span::new(lexer.span().start, lexer.span().end);
        match result {
            Ok(Tok::Newline) => nl_pending = true,
            Ok(Tok::BlockComment(had_newline)) => nl_pending |= had_newline,
            Ok(tok) => {
                out.push(Token {
                    tok,
                    span,
                    nl_before: nl_pending,
                });
                nl_pending = false;
            }
            Err(()) => {
                return Err(Diagnostic::new(
                    span,
                    alloc::format!("unexpected character `{}`", &src[lexer.span()]),
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn keywords_vs_idents() {
        assert_eq!(
            toks("var variable exporting export"),
            [Tok::Var, Tok::Ident, Tok::Ident, Tok::Export]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(toks("1 3.5 .015 5. 0x1F 0b101"), [Tok::Number; 6]);
        // `.015` is a number but `a.length` is Ident Dot Ident
        assert_eq!(toks("a.length"), [Tok::Ident, Tok::Dot, Tok::Ident]);
        assert_eq!(
            toks("time(.1)"),
            [Tok::Ident, Tok::LParen, Tok::Number, Tok::RParen]
        );
    }

    #[test]
    fn operators_longest_match() {
        assert_eq!(toks("a<<=b"), [Tok::Ident, Tok::ShlAssign, Tok::Ident]);
        assert_eq!(toks("a<<b"), [Tok::Ident, Tok::Shl, Tok::Ident]);
        assert_eq!(toks("a<b"), [Tok::Ident, Tok::Lt, Tok::Ident]);
        assert_eq!(toks("a=>b"), [Tok::Ident, Tok::FatArrow, Tok::Ident]);
        assert_eq!(toks("a>=b"), [Tok::Ident, Tok::Ge, Tok::Ident]);
        assert_eq!(
            toks("i++ +j"),
            [Tok::Ident, Tok::PlusPlus, Tok::Plus, Tok::Ident]
        );
    }

    #[test]
    fn comments_skipped() {
        assert_eq!(
            toks("a // line comment\nb /* block\ncomment */ c /* x **/ d"),
            [Tok::Ident, Tok::Ident, Tok::Ident, Tok::Ident]
        );
    }

    #[test]
    fn block_comment_newline_counts_for_asi() {
        // a multiline comment is a line terminator (JS ASI rule)
        let tokens = lex("a /* x\ny */ b /* same line */ c").unwrap();
        assert!(tokens[1].nl_before);
        assert!(!tokens[2].nl_before);
    }

    #[test]
    fn newline_flags() {
        let tokens = lex("a\nb c").unwrap();
        assert!(!tokens[0].nl_before);
        assert!(tokens[1].nl_before);
        assert!(!tokens[2].nl_before);
    }

    #[test]
    fn unexpected_char() {
        assert!(lex("a @ b").is_err());
    }
}
