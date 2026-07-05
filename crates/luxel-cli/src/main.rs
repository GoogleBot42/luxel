//! `luxel` — desktop harness for the Luxel pattern language.
//!
//! Current subcommands:
//!   luxel parse <file>    parse a pattern and dump the AST (or report errors
//!                         with line:col, as the editor will)
//!
//! `luxel run` (headless rendering to PPM/JSON) arrives with the VM.

use std::process::ExitCode;

use luxel_core::diag::line_col;
use luxel_core::parse::parse_program;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, path] if cmd == "parse" => parse_cmd(path),
        _ => {
            eprintln!("usage: luxel parse <pattern.js>");
            ExitCode::from(2)
        }
    }
}

fn parse_cmd(path: &str) -> ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match parse_program(&src) {
        Ok(stmts) => {
            println!("{stmts:#?}");
            ExitCode::SUCCESS
        }
        Err(d) => {
            let (line, col) = line_col(&src, d.span.start);
            eprintln!("{path}:{line}:{col}: error: {}", d.message);
            ExitCode::FAILURE
        }
    }
}
