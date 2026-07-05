//! Source spans and diagnostics shared by the lexer, parser, and (later) VM
//! runtime errors — the editor needs byte-accurate spans for squiggles.

use alloc::string::String;

/// Byte range into the original source text.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span {
            start: start as u32,
            end: end as u32,
        }
    }

    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start,
            end: other.end,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            span,
            message: message.into(),
        }
    }
}

/// 1-based (line, column) of a byte offset, for human-facing error output.
pub fn line_col(src: &str, offset: u32) -> (u32, u32) {
    let offset = (offset as usize).min(src.len());
    let mut line = 1;
    let mut col = 1;
    for b in src[..offset].bytes() {
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
