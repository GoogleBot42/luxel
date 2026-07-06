//! JSON snapshots of engine state, shared by every host that speaks the
//! device HTTP API (firmware, `luxel serve`) and kept in the exact shape the
//! playground's TypeScript types expect. Values are raw 16.16 (clients
//! divide by 65536), matching the wasm ABI convention.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::engine::{ControlKind, Engine};
use crate::vm::Value;

pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
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
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"kind\":\"{}\",\"label\":\"{}\",\"name\":\"{}\"}}",
            control_kind_str(c.kind),
            json_escape(&c.label),
            json_escape(&c.name)
        ));
    }
    out.push(']');
    out
}

/// `{"name":raw,"arr":[raw,…],…}` — exported vars, raw 16.16 values.
pub fn vars_json(engine: &Engine) -> String {
    let names: Vec<String> = engine.exported_vars().map(String::from).collect();
    let mut out = String::from("{");
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\":", json_escape(name)));
        match engine.var(name) {
            Some(Value::Num(v)) => out.push_str(&v.raw().to_string()),
            Some(Value::Arr(_)) => {
                out.push('[');
                for (j, v) in engine.var_array(name).unwrap_or(&[]).iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    out.push_str(&v.num().raw().to_string());
                }
                out.push(']');
            }
            _ => out.push_str("null"),
        }
    }
    out.push('}');
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
            out.push(',');
        }
        out.push_str(&format!("\"{}\":", json_escape(name)));
        match engine.set_control(name, &[]) {
            Some(v) => out.push_str(&v.raw().to_string()),
            None => out.push_str("null"),
        }
    }
    out.push('}');
    out
}
