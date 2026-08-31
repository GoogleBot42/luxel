//! JSON snapshots of engine state, shared by every host that speaks the
//! device HTTP API (firmware, `luxel serve`) and kept in the exact shape the
//! playground's TypeScript types expect. Values are raw 16.16 (clients
//! divide by 65536), matching the wasm ABI convention.

use alloc::string::String;
use alloc::vec::Vec;

use crate::engine::{ControlKind, Engine};
use crate::vm::Value;

/// Appends `s` to `out` — deliberately not inlined. `String::push_str`
/// inlines a reserve-and-copy at every call site, and these builders have
/// hundreds of them; that inlining costs more image than the `core::fmt`
/// machinery this module exists to avoid (measured on board-c6-devkit).
#[inline(never)]
pub fn push_piece(out: &mut String, s: &str) {
    out.push_str(s);
}

/// Decimal digits of `v`. Equivalent to `push_str(&format!("{v}"))`, but
/// every `format!` call site instantiates core::fmt machinery the firmware
/// cannot afford (~25 KB across the JSON builders, measured).
pub fn push_u32(out: &mut String, v: u32) {
    let mut rev = [0u8; 10];
    let mut n = 0;
    let mut x = v;
    loop {
        rev[n] = b'0' + (x % 10) as u8;
        x /= 10;
        n += 1;
        if x == 0 {
            break;
        }
    }
    while n > 0 {
        n -= 1;
        out.push(rev[n] as char);
    }
}

/// Decimal digits of `v`, with a leading `-` when negative.
pub fn push_i32(out: &mut String, v: i32) {
    if v < 0 {
        out.push('-');
    }
    push_u32(out, v.unsigned_abs());
}

/// [push_u32] for the wide values (millisecond clocks, epoch seconds).
pub fn push_u64(out: &mut String, v: u64) {
    let mut rev = [0u8; 20];
    let mut n = 0;
    let mut x = v;
    loop {
        rev[n] = b'0' + (x % 10) as u8;
        x /= 10;
        n += 1;
        if x == 0 {
            break;
        }
    }
    while n > 0 {
        n -= 1;
        out.push(rev[n] as char);
    }
}

/// [push_i32]'s wide twin.
pub fn push_i64(out: &mut String, v: i64) {
    if v < 0 {
        out.push('-');
    }
    push_u64(out, v.unsigned_abs());
}

/// Lowercase hex digits of `v`, zero-padded to at least `width` digits.
pub fn push_hex(out: &mut String, v: u32, width: usize) {
    let mut rev = [0u8; 8];
    let mut n = 0;
    let mut x = v;
    loop {
        let d = (x & 0xf) as u8;
        rev[n] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        x >>= 4;
        n += 1;
        if x == 0 {
            break;
        }
    }
    for _ in n..width {
        out.push('0');
    }
    while n > 0 {
        n -= 1;
        out.push(rev[n] as char);
    }
}

pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => push_piece(&mut out, "\\\""),
            '\\' => push_piece(&mut out, "\\\\"),
            '\n' => push_piece(&mut out, "\\n"),
            '\r' => push_piece(&mut out, "\\r"),
            '\t' => push_piece(&mut out, "\\t"),
            c if (c as u32) < 0x20 => {
                push_piece(&mut out, "\\u");
                push_hex(&mut out, c as u32, 4);
            }
            c => out.push(c),
        }
    }
    out
}

pub fn control_kind_str(k: ControlKind) -> &'static str {
    match k {
        ControlKind::Slider => "slider",
        ControlKind::HsvPicker => "hsvPicker",
        ControlKind::RgbPicker => "rgbPicker",
        ControlKind::Toggle => "toggle",
        ControlKind::Trigger => "trigger",
        ControlKind::InputNumber => "inputNumber",
        ControlKind::ShowNumber => "showNumber",
        ControlKind::Gauge => "gauge",
    }
}

/// `[{"kind":"slider","label":"Speed","name":"sliderSpeed"},…]`
pub fn controls_json(engine: &Engine) -> String {
    let mut out = String::from("[");
    for (i, c) in engine.controls().iter().enumerate() {
        if i > 0 {
            push_piece(&mut out, ",");
        }
        push_piece(&mut out, "{\"kind\":\"");
        push_piece(&mut out, control_kind_str(c.kind));
        push_piece(&mut out, "\",\"label\":\"");
        push_piece(&mut out, &json_escape(&c.label));
        push_piece(&mut out, "\",\"name\":\"");
        push_piece(&mut out, &json_escape(&c.name));
        push_piece(&mut out, "\"}");
    }
    push_piece(&mut out, "]");
    out
}

/// `{"name":raw,"arr":[raw,…],…}` — exported vars, raw 16.16 values.
pub fn vars_json(engine: &Engine) -> String {
    let names: Vec<String> = engine.exported_vars().map(String::from).collect();
    let mut out = String::from("{");
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            push_piece(&mut out, ",");
        }
        push_piece(&mut out, "\"");
        push_piece(&mut out, &json_escape(name));
        push_piece(&mut out, "\":");
        match engine.var(name) {
            Some(Value::Num(v)) => push_i32(&mut out, v.raw()),
            Some(Value::Arr(_)) => {
                push_piece(&mut out, "[");
                for (j, v) in engine.var_array(name).unwrap_or(&[]).iter().enumerate() {
                    if j > 0 {
                        push_piece(&mut out, ",");
                    }
                    push_i32(&mut out, v.num().raw());
                }
                push_piece(&mut out, "]");
            }
            _ => push_piece(&mut out, "null"),
        }
    }
    push_piece(&mut out, "}");
    out
}

/// `{"showFps":raw,…}` — current display values of showNumber/gauge
/// controls (invokes them, so needs `&mut`).
pub fn readouts_json(engine: &mut Engine) -> String {
    let names: Vec<String> = engine
        .controls()
        .iter()
        .filter(|c| matches!(c.kind, ControlKind::ShowNumber | ControlKind::Gauge))
        .map(|c| c.name.clone())
        .collect();
    let mut out = String::from("{");
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            push_piece(&mut out, ",");
        }
        push_piece(&mut out, "\"");
        push_piece(&mut out, &json_escape(name));
        push_piece(&mut out, "\":");
        match engine.set_control(name, &[]) {
            Some(v) => push_i32(&mut out, v.raw()),
            None => push_piece(&mut out, "null"),
        }
    }
    push_piece(&mut out, "}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built(f: impl FnOnce(&mut String)) -> String {
        let mut out = String::new();
        f(&mut out);
        out
    }

    #[test]
    fn push_ints_match_fmt() {
        for v in [0u32, 1, 9, 10, 65536, 4_294_967_295] {
            assert_eq!(built(|o| push_u32(o, v)), alloc::format!("{v}"));
        }
        for v in [0i32, 1, -1, 10, -99999, i32::MIN, i32::MAX] {
            assert_eq!(built(|o| push_i32(o, v)), alloc::format!("{v}"));
        }
        for v in [0u64, 1, 10, u32::MAX as u64 + 1, u64::MAX] {
            assert_eq!(built(|o| push_u64(o, v)), alloc::format!("{v}"));
        }
        for v in [0i64, 1, -1, 1_700_000_000, i64::MIN, i64::MAX] {
            assert_eq!(built(|o| push_i64(o, v)), alloc::format!("{v}"));
        }
    }

    #[test]
    fn push_hex_matches_fmt() {
        for (v, w) in [
            (0u32, 4usize),
            (1, 4),
            (0x1f, 2),
            (0xdead_beef, 8),
            (0xdead_beef, 2),
            (5, 0),
            (0x10, 8),
            (u32::MAX, 4),
        ] {
            assert_eq!(built(|o| push_hex(o, v, w)), alloc::format!("{v:0w$x}"));
        }
    }

    #[test]
    fn escapes_control_chars() {
        assert_eq!(json_escape("a\u{1}b\u{1f}"), "a\\u0001b\\u001f");
        assert_eq!(json_escape("\"\\\n\r\t"), "\\\"\\\\\\n\\r\\t");
    }
}
