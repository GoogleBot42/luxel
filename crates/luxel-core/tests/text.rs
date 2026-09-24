//! Text in patterns (Gitea #483 C1, #484 C2, #485 C3-core).
//!
//! The parser half (a string literal is legal ONLY in a text builtin's
//! argument list, and lowers to a number), the format half (it reuses the
//! assert-message table, so LXBC does not bump), and the runtime half (the
//! five builtins against `luxel_core::text`'s own kernels).

use luxel_core::bytecode::{deserialize, deserialize_lean, serialize, FORMAT_VERSION};
use luxel_core::compile::compile;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::outpipe::GridMap;
use luxel_core::text::{self, Align, Font};

/// Wrap a body in the whole-frame entry — the only place a draw op works.
fn rf(body: &str) -> String {
    format!("export function renderFrame() {{\n{body}\n}}")
}

/// One frame of `src` on a `w×h` row-major grid.
fn grid_frame(src: &str, w: u16, h: u16) -> Vec<[u8; 3]> {
    let mut e = Engine::new(src, w as u32 * h as u32, 1).expect("compile");
    e.set_grid_map(w, h);
    let out = e.frame(Fx::from_int(16)).to_vec();
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
    out
}

/// One frame of `src` on an `n`-pixel strip (no grid at all).
fn strip_frame(src: &str, n: u32) -> Vec<[u8; 3]> {
    let mut e = Engine::new(src, n, 1).expect("compile");
    let out = e.frame(Fx::from_int(16)).to_vec();
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
    out
}

/// What `text::draw_aligned` alone puts on the same grid — the reference
/// every VM-path assertion below compares against, so the builtin and the
/// kernel can never drift.
fn reference(w: u16, h: u16, x: i32, y: i32, s: &str, f: Font, rgb: [u8; 3], a: Align) -> Vec<[u8; 3]> {
    let g = GridMap {
        w,
        h,
        serpentine: false,
    };
    let mut px = vec![[0u8; 3]; w as usize * h as usize];
    text::draw_aligned(&mut px, &g, x, y, s, f, rgb, a);
    px
}

// ------------------------------------------------------------------ C1

#[test]
fn string_literals_are_legal_only_as_text_builtin_arguments() {
    for ok in [
        r#"drawText("HI", 0, 0)"#,
        r#"drawText("HI", 0, 0, 1)"#,
        r#"textWidth("HI")"#,
        r#"font("tiny")"#,
        // a literal beside ordinary expressions in the same call
        r#"drawText("HI", gridWidth() / 2, 1, 1)"#,
    ] {
        compile(&rf(ok)).unwrap_or_else(|d| panic!("{ok}: {}", d.message));
    }
    for bad in [
        r#"var s = "hi""#,
        r#"hsv("hi", 1, 1)"#,
        r#"drawNumber("hi", 0, 0, 1, 0)"#,
        r#"textSlot("hi")"#,
        r#"fill("hi")"#,
        r#"var a = ["hi"]"#,
        r#"drawText(1 + "hi", 0, 0)"#,
    ] {
        let d = compile(&rf(bad)).expect_err(bad);
        assert!(
            d.message.contains("no string values"),
            "{bad}: {}",
            d.message
        );
        assert!(
            d.message.contains("drawText()"),
            "the error should name the text builtins: {}",
            d.message
        );
    }
    // `assert` keeps its own accepting site
    compile("assert(1, \"still fine\")\nexport function render(i) { hsv(0,0,0) }").unwrap();
}

#[test]
fn literals_intern_into_the_message_table_and_dedup() {
    let prog = compile(&rf(r#"
        drawText("HI", 0, 0)
        drawText("HI", 0, 8)
        drawText("BYE", 0, 16)
    "#))
    .unwrap();
    assert_eq!(prog.assert_msgs, vec!["HI".to_string(), "BYE".to_string()]);

    // …and shares the table with `assert()`, which is the whole reason
    // there is no new section and no format bump
    let prog = compile(&format!(
        "assert(1, \"HI\")\n{}",
        rf(r#"drawText("HI", 0, 0)"#)
    ))
    .unwrap();
    assert_eq!(prog.assert_msgs, vec!["HI".to_string()]);
}

#[test]
fn a_long_literal_truncates_on_a_char_boundary() {
    // one ASCII byte then 3-byte characters, so `intern_msg`'s 252-byte cut
    // lands INSIDE a character and has to walk back to 250
    let long: String = core::iter::once('x')
        .chain(core::iter::repeat_n('\u{2603}', 120))
        .collect();
    let prog = compile(&rf(&format!("drawText(\"{long}\", 0, 0)"))).unwrap();
    let m = &prog.assert_msgs[0];
    assert_eq!(m.len(), 253, "truncated to a char boundary, then `...`");
    assert!(m.ends_with("..."), "{m}");
    // still valid UTF-8 and a whole number of snowmen
    assert_eq!(m.chars().filter(|&c| c == '\u{2603}').count(), 83);
    // the blob round-trips (str8 would reject a 256-byte string)
    let blob = serialize(&prog).unwrap();
    assert_eq!(deserialize(&blob).unwrap().assert_msgs, prog.assert_msgs);
}

#[test]
fn text_costs_no_format_bump_and_survives_a_lean_decode() {
    assert_eq!(FORMAT_VERSION, 6, "C1 must not bump the LXBC version");
    let prog = compile(&rf(r#"
        rgb(1, 1, 1)
        font("tiny")
        drawText("HI", 0, 0)
    "#))
    .unwrap();
    let blob = serialize(&prog).unwrap();
    assert_eq!(blob[4], FORMAT_VERSION as u8);
    // the firmware's decode path drops debug info but keeps the message
    // table — which is where a text literal now lives
    let lean = deserialize_lean(&blob).unwrap();
    assert_eq!(lean.assert_msgs, vec!["tiny".to_string(), "HI".to_string()]);

    let mut e = Engine::from_program(lean, 64, 1);
    e.set_grid_map(8, 8);
    let px = e.frame(Fx::from_int(16)).to_vec();
    assert!(e.last_error.is_none(), "{:?}", e.last_error);
    assert_eq!(px, reference(8, 8, 0, 0, "HI", Font::Tiny, [255, 255, 255], Align::Left));
}

#[test]
fn a_blob_compiled_before_text_existed_still_decodes_and_runs() {
    // tests/data/pre-text-v6.lxbc was produced by `luxel compile` on the
    // commit before the text builtins landed (source beside it). The point
    // is that appending builtin ids and reusing the message table left
    // every stored blob on every device readable (Gitea #643).
    let blob = include_bytes!("data/pre-text-v6.lxbc");
    for prog in [deserialize(blob).unwrap(), deserialize_lean(blob).unwrap()] {
        assert_eq!(prog.assert_msgs, vec!["needs pixels".to_string()]);
        let mut e = Engine::from_program(prog, 16, 1);
        for _ in 0..3 {
            e.frame(Fx::from_int(16));
        }
        assert!(e.last_error.is_none(), "{:?}", e.last_error);
    }
}

// ------------------------------------------------------------- C2 / C3

#[test]
fn draw_text_paints_the_kernels_pixels_in_the_brush_colour() {
    let px = grid_frame(&rf(r#"
        rgb(1, 0, 0)
        drawText("HI", 1, 2)
    "#), 16, 12);
    assert_eq!(px, reference(16, 12, 1, 2, "HI", Font::Regular, [255, 0, 0], Align::Left));
    // and something was actually drawn
    assert!(px.iter().any(|&p| p == [255, 0, 0]));
}

#[test]
fn draw_text_returns_the_advance_and_honours_align() {
    // the return value is textWidth's answer
    let px = grid_frame(&rf(r#"
        rgb(0, 1, 0)
        var w = drawText("HI", 0, 0)
        if (w == textWidth("HI")) { drawText("HI", 0, 7) }
    "#), 16, 16);
    assert!(px.iter().filter(|&&p| p == [0, 255, 0]).count() > 0);
    assert_eq!(px[7 * 16..8 * 16], px[0..16], "the second run drew too");

    for (code, align) in [(0, Align::Left), (1, Align::Center), (2, Align::Right)] {
        let px = grid_frame(
            &rf(&format!("rgb(0, 0, 1)\ndrawText(\"HI\", 8, 1, {code})")),
            16,
            12,
        );
        assert_eq!(
            px,
            reference(16, 12, 8, 1, "HI", Font::Regular, [0, 0, 255], align),
            "align {code}"
        );
    }
}

#[test]
fn font_is_modal_and_ignores_an_unknown_name() {
    let px = grid_frame(&rf(r#"
        rgb(1, 1, 1)
        font("tiny")
        font("nonesuch")
        drawText("HI", 0, 0)
    "#), 16, 12);
    assert_eq!(px, reference(16, 12, 0, 0, "HI", Font::Tiny, [255, 255, 255], Align::Left));

    // font() returns the ACTIVE face's index, so a pattern can read it
    let px = grid_frame(&rf(r#"
        rgb(1, 1, 1)
        font("large")
        setPixel(font(""))
    "#), 8, 8);
    assert_eq!(px[2], [255, 255, 255]);
    assert_eq!(px[0], [0, 0, 0]);
}

#[test]
fn text_width_measures_without_a_grid_and_draw_is_a_silent_no_op() {
    // textWidth is pure arithmetic: it answers on a bare strip
    let px = strip_frame(&rf(r#"
        rgb(1, 1, 1)
        setPixel(textWidth("HI") - 1)
    "#), 16);
    assert_eq!(px[11], [255, 255, 255], "5x7 advance 6, so 2 chars = 12");

    // drawText draws nothing and returns 0 — no error, like every bulk op
    let px = strip_frame(&rf(r#"
        rgb(1, 1, 1)
        if (drawText("HI", 0, 0) == 0) { setPixel(3) }
    "#), 16);
    assert_eq!(px[3], [255, 255, 255]);
    assert_eq!(px.iter().filter(|&&p| p != [0, 0, 0]).count(), 1);
}

#[test]
fn draw_number_formats_fixed_point() {
    for (expr, want) in [
        ("drawNumber(42, 0, 0, 1, 0)", "42"),
        ("drawNumber(42, 0, 0, 4, 0)", "0042"),
        ("drawNumber(-1.5, 0, 0, 2, 1)", "-01.5"),
        ("drawNumber(1/3, 0, 0, 1, 3)", "0.333"),
    ] {
        let px = grid_frame(&rf(&format!("rgb(1, 1, 1)\n{expr}")), 48, 8);
        assert_eq!(
            px,
            reference(48, 8, 0, 0, want, Font::Regular, [255, 255, 255], Align::Left),
            "{expr}"
        );
    }
}

#[test]
fn text_slots_reach_a_pattern_through_a_handle() {
    text::set_slot(3, "HI");
    let px = grid_frame(&rf(r#"
        rgb(1, 1, 1)
        drawText(textSlot(3), 0, 0)
    "#), 16, 12);
    assert_eq!(px, reference(16, 12, 0, 0, "HI", Font::Regular, [255, 255, 255], Align::Left));

    // textWidth takes the same handle
    let px = grid_frame(&rf(r#"
        rgb(1, 1, 1)
        setPixel(textWidth(textSlot(3)))
    "#), 16, 12);
    assert_eq!(px[12], [255, 255, 255]);

    // an empty or out-of-range slot draws nothing and measures 0
    text::set_slot(3, "");
    let px = grid_frame(&rf(r#"
        rgb(1, 1, 1)
        drawText(textSlot(3), 0, 0)
        drawText(textSlot(99), 0, 4)
        setPixel(textWidth(textSlot(3)))
    "#), 16, 12);
    assert_eq!(px[0], [255, 255, 255]);
    assert_eq!(px.iter().filter(|&&p| p != [0, 0, 0]).count(), 1);
}

#[test]
fn a_bogus_handle_draws_nothing_rather_than_erroring() {
    // a number that is not a literal's index and not a slot: patterns can
    // pass anything, and a draw op never errors
    let px = grid_frame(&rf(r#"
        rgb(1, 1, 1)
        drawText(1234, 0, 0)
        drawText(-99999, 0, 4)
        setPixel(0)
    "#), 16, 12);
    assert_eq!(px.iter().filter(|&&p| p != [0, 0, 0]).count(), 1);
}
