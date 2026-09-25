//! JSON snapshots of engine state, shared by every host that speaks the
//! device HTTP API (firmware, `luxel serve`) and kept in the exact shape the
//! playground's TypeScript types expect. Values are raw 16.16 (clients
//! divide by 65536), matching the wasm ABI convention.

use alloc::string::String;
use alloc::vec::Vec;

use crate::engine::{ControlKind, Engine};
use crate::vm::Value;

/// Where a JSON builder puts its bytes.
///
/// Every builder in this module — and `scene::push_json`, `caps::push_json`,
/// the firmware's `/api/status` — writes through this ONE object-safe trait,
/// so a body can land in a plain [`String`] (the host mirror, a small
/// fixed-shape reply) or in [`Chunks`] (a device response that must never ask
/// the allocator for a contiguous block) without either builder being written
/// or compiled twice. Deliberately `&mut dyn Sink` rather than a generic:
/// `scene::push_json` alone is several KB of Xtensa, and the firmware pays
/// for every monomorphization (Gitea #167, #753).
pub trait Sink {
    /// Append `s`. Infallible by signature — a sink that can fail records it
    /// and keeps going (see [`Chunks::ok`]), because threading a `Result`
    /// through several hundred push sites is a diff nobody can review.
    fn put(&mut self, s: &str);
}

impl Sink for String {
    fn put(&mut self, s: &str) {
        self.push_str(s);
    }
}

/// Bytes in one [`Chunks`] segment. The whole point of the type: this, not
/// the response size, is the largest contiguous block a generated body ever
/// asks the allocator for.
///
/// 256 B keeps the ask an order of magnitude under what a doubling `String`
/// needed for the bodies that panicked a device (2,560–2,688 B, Gitea #728)
/// while keeping per-chunk allocator overhead and the number of socket
/// writes down. A 3 KB `/api/status` body is 12 segments.
pub const CHUNK: usize = 256;

/// A JSON body held as a list of [`CHUNK`]-sized segments instead of one
/// contiguous `String` (Gitea #753).
///
/// Why this exists: a `String` grows by doubling, and a doubling step holds
/// the old buffer and the new one at once — so a 2.7 KB response wanted a
/// contiguous 2.7 KB block on a board whose largest free block was smaller
/// than that, and `GET /api/status` rebooted the device three times in one
/// session with nothing but the console's poll as the trigger. Segmenting
/// the body removes the contiguous requirement outright: peak demand is
/// [`CHUNK`] plus the segment index, on every board, with no PSRAM anywhere
/// in the story (the Pixelblaze v3 has none and stays first-class, Gitea
/// #752).
///
/// The response streams straight out of the segments ([`Chunks::parts`]), so
/// the body is never flattened, and [`Chunks::len`] is the exact byte count
/// by construction — a `Content-Length` a two-pass measure-then-emit could
/// get wrong (`tools/wire-check.sh`) cannot be wrong here.
///
/// Every allocation it makes is fallible: a segment or index it cannot have
/// sets [`Chunks::ok`] false and the caller answers 503 rather than taking
/// the allocator's panic.
pub struct Chunks {
    parts: Vec<String>,
    len: usize,
    ok: bool,
}

impl Default for Chunks {
    fn default() -> Self {
        Chunks::new()
    }
}

impl Chunks {
    pub fn new() -> Self {
        Chunks {
            parts: Vec::new(),
            len: 0,
            ok: true,
        }
    }

    /// [`Chunks::new`] with the segment INDEX sized up front from a byte
    /// hint — `scene::json_bound`, `scenestore::list_hint`, the previous
    /// body. Only the index is reserved (a handful of pointers); segments
    /// are still taken one at a time, so an over-generous hint costs a few
    /// bytes, never a refused response.
    pub fn with_hint(bytes: usize) -> Self {
        let mut c = Chunks::new();
        if c.parts.try_reserve(bytes / CHUNK + 1).is_err() {
            c.ok = false;
        }
        c
    }

    /// Bytes written — the exact `Content-Length`.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// False once any allocation failed, i.e. the body is INCOMPLETE and
    /// must not go on the wire.
    pub fn ok(&self) -> bool {
        self.ok
    }

    /// The segments, in order, for a response to write out.
    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    /// The segments, owned — so a response writer can free each one the
    /// moment it is on the wire instead of holding the whole body until the
    /// response ends.
    pub fn into_parts(self) -> Vec<String> {
        self.parts
    }

    /// Whole body as one `String` — for host tests and the mirror, never the
    /// firmware's response path (it is exactly the contiguous allocation
    /// this type exists to avoid).
    pub fn to_string_lossy(&self) -> String {
        let mut s = String::new();
        for p in &self.parts {
            s.push_str(p);
        }
        s
    }

    /// A fresh empty segment, or `false` having poisoned the builder.
    fn grow(&mut self) -> bool {
        let mut seg = String::new();
        if seg.try_reserve_exact(CHUNK).is_err() || self.parts.try_reserve(1).is_err() {
            self.ok = false;
            return false;
        }
        self.parts.push(seg);
        true
    }
}

impl Sink for Chunks {
    fn put(&mut self, s: &str) {
        if !self.ok {
            return;
        }
        let mut rest = s;
        while !rest.is_empty() {
            let room = match self.parts.last() {
                Some(seg) => CHUNK - seg.len(),
                None => 0,
            };
            // Never write past CHUNK: the segment was reserved for exactly
            // that, and overrunning it would make the segment REALLOCATE —
            // the contiguous doubling this type exists to remove.
            let mut take = rest.len().min(room);
            while take > 0 && !rest.is_char_boundary(take) {
                take -= 1;
            }
            if take == 0 {
                // no segment yet, it is full, or the next char straddles the
                // end of it
                if !self.grow() {
                    return;
                }
                continue;
            }
            // `last` exists whenever room > 0
            if let Some(seg) = self.parts.last_mut() {
                seg.push_str(&rest[..take]);
            }
            self.len += take;
            rest = &rest[take..];
        }
    }
}

/// Appends `s` to `out` — deliberately not inlined. `String::push_str`
/// inlines a reserve-and-copy at every call site, and these builders have
/// hundreds of them; that inlining costs more image than the `core::fmt`
/// machinery this module exists to avoid (measured on board-c6-devkit).
#[inline(never)]
pub fn push_piece(out: &mut dyn Sink, s: &str) {
    out.put(s);
}

/// Decimal digits of `v`. Equivalent to `push_str(&format!("{v}"))`, but
/// every `format!` call site instantiates core::fmt machinery the firmware
/// cannot afford (~25 KB across the JSON builders, measured).
pub fn push_u32(out: &mut dyn Sink, v: u32) {
    let mut buf = [0u8; 10];
    let mut n = buf.len();
    let mut x = v;
    loop {
        n -= 1;
        buf[n] = b'0' + (x % 10) as u8;
        x /= 10;
        if x == 0 {
            break;
        }
    }
    out.put(as_ascii(&buf[n..]));
}

/// A digit run built by the integer pushers — ASCII by construction, so the
/// utf-8 check is a branch the firmware should not pay for on every number.
fn as_ascii(b: &[u8]) -> &str {
    // SAFETY-free: `from_utf8` on pure ASCII cannot fail, and the fallback
    // keeps this function `unsafe`-free (the firmware forbids `unsafe` here).
    core::str::from_utf8(b).unwrap_or("")
}

/// Decimal digits of `v`, with a leading `-` when negative.
pub fn push_i32(out: &mut dyn Sink, v: i32) {
    if v < 0 {
        out.put("-");
    }
    push_u32(out, v.unsigned_abs());
}

/// `(v / 10, v % 10)`, deliberately opaque to the optimiser. Dividing a
/// `u64` by the literal 10 makes LLVM emit a 64-bit magic multiply AND
/// unroll all twenty digit positions: 1,909 B of Xtensa for a formatter
/// that runs a handful of times per HTTP request (Gitea #312). Behind a
/// call the sequence is emitted once and the digit loop stays a loop.
#[inline(never)]
fn divmod10_u64(v: u64) -> (u64, u8) {
    let q = v / 10;
    (q, (v - q * 10) as u8)
}

/// [push_u32] for the wide values (millisecond clocks, epoch seconds).
pub fn push_u64(out: &mut dyn Sink, v: u64) {
    let mut buf = [0u8; 20];
    let mut n = buf.len();
    let mut x = v;
    loop {
        let (q, d) = divmod10_u64(x);
        n -= 1;
        buf[n] = b'0' + d;
        x = q;
        if x == 0 {
            break;
        }
    }
    out.put(as_ascii(&buf[n..]));
}

/// [push_i32]'s wide twin.
pub fn push_i64(out: &mut dyn Sink, v: i64) {
    if v < 0 {
        out.put("-");
    }
    push_u64(out, v.unsigned_abs());
}

/// Lowercase hex digits of `v`, zero-padded to at least `width` digits.
pub fn push_hex(out: &mut dyn Sink, v: u32, width: usize) {
    let mut buf = [b'0'; 8];
    let mut n = buf.len();
    let mut x = v;
    loop {
        let d = (x & 0xf) as u8;
        n -= 1;
        buf[n] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        x >>= 4;
        if x == 0 {
            break;
        }
    }
    // A width wider than the buffer (no caller does this today, but the
    // formatter this replaced allowed it) pads ahead of the digits.
    let mut extra = width.saturating_sub(buf.len());
    while extra > 0 {
        out.put("0");
        extra -= 1;
    }
    while buf.len() - n < width.min(buf.len()) {
        n -= 1;
    }
    out.put(as_ascii(&buf[n..]));
}

/// A response body with `hint` bytes reserved in ONE fallible allocation, or
/// `None` when the heap cannot hand over that much contiguous memory.
///
/// For a body whose size is KNOWN and small — a scene blob about to go to
/// flash, a fixed-shape reply. A generated response body that can grow past
/// a few hundred bytes wants [`Chunks`] instead: the point of #753 is that
/// such a body should never need a contiguous block at all.
pub fn try_body(hint: usize) -> Option<String> {
    let mut out = String::new();
    if out.try_reserve_exact(hint).is_err() {
        return None;
    }
    Some(out)
}

/// Bytes [`json_escape`] would produce, without building it — the measuring
/// half of a fallible reservation, and what `scene::json_bound` counts with.
pub fn json_escape_len(s: &str) -> usize {
    let mut n = 0;
    for c in s.chars() {
        n += match c {
            '"' | '\\' | '\n' | '\r' | '\t' => 2,
            c if (c as u32) < 0x20 => 6,
            c => c.len_utf8(),
        };
    }
    n
}

/// [`json_escape`] straight into a sink — no intermediate `String`, so a
/// long `vmerr` or pattern name is not its own heap allocation on the
/// response path.
pub fn push_escaped(out: &mut dyn Sink, s: &str) {
    // Runs of ordinary characters go out in ONE put; only an escape breaks
    // the run. Nearly every string here escapes nothing.
    let mut start = 0;
    for (i, c) in s.char_indices() {
        let esc = match c {
            '"' => "\\\"",
            '\\' => "\\\\",
            '\n' => "\\n",
            '\r' => "\\r",
            '\t' => "\\t",
            c if (c as u32) < 0x20 => "",
            _ => continue,
        };
        if start < i {
            out.put(&s[start..i]);
        }
        if esc.is_empty() {
            out.put("\\u");
            push_hex(out, c as u32, 4);
        } else {
            out.put(esc);
        }
        start = i + c.len_utf8();
    }
    if start < s.len() {
        out.put(&s[start..]);
    }
}

pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    push_escaped(&mut out, s);
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
        push_escaped(&mut out, &c.label);
        push_piece(&mut out, "\",\"name\":\"");
        push_escaped(&mut out, &c.name);
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
        push_escaped(&mut out, name);
        push_piece(&mut out, "\":");
        match engine.var(name) {
            Some(Value::Num(v)) => push_i32(&mut out, v.raw()),
            Some(Value::Arr(_)) => {
                push_piece(&mut out, "[");
                for (j, v) in engine.var_array(name).into_iter().flat_map(|a| a.iter()).enumerate() {
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
        push_escaped(&mut out, name);
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

    /// The measuring half of the fallible reservation has to agree with the
    /// builder EXACTLY, or `scene::json_bound` is not a bound (Gitea #728).
    #[test]
    fn escape_len_matches_escape() {
        for s in [
            "",
            "plain",
            "a\u{1}b\u{1f}",
            "\"\\\n\r\t",
            "emoji \u{1f680} and accents éà",
            "0123456789012345678901234567890123456789012345678901234567890123",
        ] {
            assert_eq!(json_escape_len(s), json_escape(s).len(), "{s:?}");
        }
    }

    #[test]
    fn try_body_reserves_or_declines() {
        let b = try_body(1024).expect("the host can spare 1 KiB");
        assert!(b.capacity() >= 1024);
        assert!(b.is_empty());
        // a reservation no allocator can satisfy is an answer, not a panic
        assert!(try_body(usize::MAX / 2).is_none());
    }

    // ---- the segmented body (Gitea #753) ----

    /// The property the whole design rests on: whatever goes in, the bytes
    /// that come back out are byte-identical to a plain `String` build, and
    /// [`Chunks::len`] is exactly that many bytes — so a `Content-Length`
    /// taken from it cannot desync from the wire.
    #[test]
    fn chunks_hold_the_same_bytes_a_string_would() {
        for body in [
            String::new(),
            String::from("x"),
            "a".repeat(CHUNK - 1),
            "b".repeat(CHUNK),
            "c".repeat(CHUNK + 1),
            "d".repeat(CHUNK * 4 + 7),
            // multi-byte characters straddling a segment boundary
            "é".repeat(CHUNK),
            "\u{1f680}".repeat(CHUNK),
        ] {
            let mut c = Chunks::new();
            // fed in awkward slices, the way a builder's many small pushes do
            let mut rest = body.as_str();
            while !rest.is_empty() {
                let mut n = rest.len().min(37);
                while !rest.is_char_boundary(n) {
                    n -= 1;
                }
                c.put(&rest[..n]);
                rest = &rest[n..];
            }
            assert!(c.ok());
            assert_eq!(c.len(), body.len(), "{} B body", body.len());
            assert_eq!(c.to_string_lossy(), body);
            assert_eq!(c.parts().iter().map(|p| p.len()).sum::<usize>(), c.len());
        }
    }

    /// No segment may exceed [`CHUNK`]: a segment that overran its
    /// `try_reserve_exact` would reallocate, which is the contiguous
    /// doubling this type exists to remove.
    #[test]
    fn no_segment_outgrows_its_reservation() {
        let mut c = Chunks::with_hint(4096);
        for i in 0..500u32 {
            push_piece(&mut c, "{\"n\":");
            push_u32(&mut c, i);
            push_piece(&mut c, "},");
        }
        push_escaped(&mut c, "a \"quoted\" ünicode \u{1} tail");
        assert!(c.ok());
        for p in c.parts() {
            assert!(p.len() <= CHUNK, "{} B segment over CHUNK", p.len());
            assert!(p.capacity() == CHUNK, "segment reallocated: cap {}", p.capacity());
        }
        // and the whole thing still reads back as what was pushed
        let s = c.to_string_lossy();
        assert!(s.starts_with("{\"n\":0},{\"n\":1},"));
        assert!(s.ends_with("a \\\"quoted\\\" ünicode \\u0001 tail"));
        assert_eq!(s.len(), c.len());
    }

    /// A failed segment allocation POISONS the builder — it does not panic,
    /// and it does not quietly hand back a truncated body that a caller
    /// would put a full `Content-Length` on.
    #[test]
    fn a_failed_segment_poisons_the_builder() {
        // the index reservation is the one a host can actually fail
        let c = Chunks::with_hint(usize::MAX / 2);
        assert!(!c.ok());
        // and a poisoned builder swallows further writes rather than lying
        let mut c = c;
        push_piece(&mut c, "ignored");
        assert_eq!(c.len(), 0);
        assert!(!c.ok());
    }

    /// `push_escaped` must produce exactly what `json_escape` does — it is
    /// the streaming twin, and `json_escape_len` measures both.
    #[test]
    fn push_escaped_matches_json_escape() {
        for s in [
            "",
            "plain",
            "a\u{1}b\u{1f}",
            "\"\\\n\r\t",
            "emoji \u{1f680} and accents éà",
            "trailing\\",
            "\u{0}start",
        ] {
            let mut c = Chunks::new();
            push_escaped(&mut c, s);
            assert_eq!(c.to_string_lossy(), json_escape(s), "{s:?}");
            assert_eq!(c.len(), json_escape_len(s), "{s:?}");
        }
    }
}
