//! The smallest JSON writer and reader that covers the sidecar this tool
//! emits — deliberately dependency-free.
//!
//! Why not serde: `storegen`'s dependency list is its correctness argument.
//! Every crate in it is pinned to exactly what `firmware/Cargo.toml` pins,
//! because the key area this writes must be byte-identical to what the
//! device writes. A serialization crate the firmware does not use would
//! weaken that claim for no gain — the sidecar is a flat object of strings,
//! numbers and arrays of flat objects, and both directions of that fit in
//! one screen each.

use std::collections::BTreeMap;
use std::fmt::Write as _;

// ------------------------------------------------------------------ write

/// A JSON value, only as rich as the sidecar needs.
pub enum J {
    Num(u64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    pub fn s(v: &str) -> J {
        J::Str(v.to_string())
    }

    fn render(&self, out: &mut String, indent: usize) {
        let pad = "  ".repeat(indent);
        let pad1 = "  ".repeat(indent + 1);
        match self {
            J::Num(n) => {
                let _ = write!(out, "{}", n);
            }
            J::Str(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c if (c as u32) < 0x20 => {
                            let _ = write!(out, "\\u{:04x}", c as u32);
                        }
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            J::Arr(items) if items.is_empty() => out.push_str("[]"),
            J::Arr(items) => {
                out.push_str("[\n");
                for (i, it) in items.iter().enumerate() {
                    out.push_str(&pad1);
                    it.render(out, indent + 1);
                    out.push_str(if i + 1 == items.len() { "\n" } else { ",\n" });
                }
                out.push_str(&pad);
                out.push(']');
            }
            J::Obj(fields) if fields.is_empty() => out.push_str("{}"),
            J::Obj(fields) => {
                out.push_str("{\n");
                for (i, (k, v)) in fields.iter().enumerate() {
                    out.push_str(&pad1);
                    J::Str(k.clone()).render(out, indent + 1);
                    out.push_str(": ");
                    v.render(out, indent + 1);
                    out.push_str(if i + 1 == fields.len() { "\n" } else { ",\n" });
                }
                out.push_str(&pad);
                out.push('}');
            }
        }
    }

    pub fn to_pretty(&self) -> String {
        let mut s = String::new();
        self.render(&mut s, 0);
        s.push('\n');
        s
    }
}

// ------------------------------------------------------------------- read

/// What comes back out. Numbers are u64 (the sidecar has no negatives and
/// no floats); anything else is a parse error.
#[derive(Debug, Clone)]
pub enum V {
    Num(u64),
    Str(String),
    Arr(Vec<V>),
    Obj(BTreeMap<String, V>),
}

impl V {
    pub fn get(&self, key: &str) -> Result<&V, String> {
        match self {
            V::Obj(m) => m.get(key).ok_or_else(|| format!("sidecar: no key {:?}", key)),
            _ => Err(format!("sidecar: expected an object to read {:?} from", key)),
        }
    }
    pub fn num(&self, key: &str) -> Result<u64, String> {
        match self.get(key)? {
            V::Num(n) => Ok(*n),
            other => Err(format!("sidecar: {:?} is {:?}, expected a number", key, other)),
        }
    }
    pub fn u32(&self, key: &str) -> Result<u32, String> {
        Ok(self.num(key)? as u32)
    }
    pub fn str(&self, key: &str) -> Result<&str, String> {
        match self.get(key)? {
            V::Str(s) => Ok(s),
            other => Err(format!("sidecar: {:?} is {:?}, expected a string", key, other)),
        }
    }
    pub fn arr(&self, key: &str) -> Result<&[V], String> {
        match self.get(key)? {
            V::Arr(a) => Ok(a),
            other => Err(format!("sidecar: {:?} is {:?}, expected an array", key, other)),
        }
    }
}

pub fn parse(text: &str) -> Result<V, String> {
    let b: Vec<char> = text.chars().collect();
    let mut at = 0usize;
    let v = value(&b, &mut at)?;
    ws(&b, &mut at);
    if at != b.len() {
        return Err(format!("sidecar: trailing junk at char {}", at));
    }
    Ok(v)
}

fn ws(b: &[char], at: &mut usize) {
    while *at < b.len() && b[*at].is_whitespace() {
        *at += 1;
    }
}

fn value(b: &[char], at: &mut usize) -> Result<V, String> {
    ws(b, at);
    match b.get(*at) {
        Some('{') => object(b, at),
        Some('[') => array(b, at),
        Some('"') => Ok(V::Str(string(b, at)?)),
        Some(c) if c.is_ascii_digit() => number(b, at),
        other => Err(format!("sidecar: unexpected {:?} at char {}", other, at)),
    }
}

fn object(b: &[char], at: &mut usize) -> Result<V, String> {
    *at += 1; // '{'
    let mut m = BTreeMap::new();
    ws(b, at);
    if b.get(*at) == Some(&'}') {
        *at += 1;
        return Ok(V::Obj(m));
    }
    loop {
        ws(b, at);
        let k = string(b, at)?;
        ws(b, at);
        if b.get(*at) != Some(&':') {
            return Err(format!("sidecar: expected ':' at char {}", at));
        }
        *at += 1;
        m.insert(k, value(b, at)?);
        ws(b, at);
        match b.get(*at) {
            Some(',') => *at += 1,
            Some('}') => {
                *at += 1;
                return Ok(V::Obj(m));
            }
            other => return Err(format!("sidecar: expected ',' or '}}', got {:?}", other)),
        }
    }
}

fn array(b: &[char], at: &mut usize) -> Result<V, String> {
    *at += 1; // '['
    let mut v = Vec::new();
    ws(b, at);
    if b.get(*at) == Some(&']') {
        *at += 1;
        return Ok(V::Arr(v));
    }
    loop {
        v.push(value(b, at)?);
        ws(b, at);
        match b.get(*at) {
            Some(',') => *at += 1,
            Some(']') => {
                *at += 1;
                return Ok(V::Arr(v));
            }
            other => return Err(format!("sidecar: expected ',' or ']', got {:?}", other)),
        }
    }
}

fn string(b: &[char], at: &mut usize) -> Result<String, String> {
    if b.get(*at) != Some(&'"') {
        return Err(format!("sidecar: expected a string at char {}", at));
    }
    *at += 1;
    let mut s = String::new();
    loop {
        match b.get(*at) {
            None => return Err("sidecar: unterminated string".into()),
            Some('"') => {
                *at += 1;
                return Ok(s);
            }
            Some('\\') => {
                *at += 1;
                match b.get(*at) {
                    Some('n') => s.push('\n'),
                    Some('r') => s.push('\r'),
                    Some('t') => s.push('\t'),
                    Some('u') => {
                        let hex: String = b.get(*at + 1..*at + 5).ok_or("sidecar: short \\u")?.iter().collect();
                        let n = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
                        s.push(char::from_u32(n).ok_or("sidecar: bad \\u")?);
                        *at += 4;
                    }
                    Some(c) => s.push(*c),
                    None => return Err("sidecar: unterminated escape".into()),
                }
                *at += 1;
            }
            Some(c) => {
                s.push(*c);
                *at += 1;
            }
        }
    }
}

fn number(b: &[char], at: &mut usize) -> Result<V, String> {
    let start = *at;
    while *at < b.len() && b[*at].is_ascii_digit() {
        *at += 1;
    }
    let s: String = b[start..*at].iter().collect();
    s.parse::<u64>().map(V::Num).map_err(|e| format!("sidecar: {}", e))
}
