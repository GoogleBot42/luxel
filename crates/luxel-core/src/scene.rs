//! Scene records — the ordered layer stack the compositor draws.
//!
//! A scene is one block of `\n`-separated lines, listed **bottom → top**
//! (the first `L` is the base). The grammar is the playlist's: a leading
//! one-letter tag, whitespace-separated fields, unknown lines ignored so an
//! older firmware survives a newer console's push. Full wire grammar in
//! `docs/spec/scenes.md`; this module is the ONE parser and the ONE
//! serializer, shared by the firmware, the `luxel serve` mirror and the
//! wasm playground so none of them can drift.
//!
//! ```text
//! S <id> <name…>
//! U <w> <h>                       the design size (before the first L)
//! L <type> <x> <y> <w> <h> <blend> <opacity> <key> <fit> <flags>
//!   N <name…>                     layer display name
//!   I <id>                        pat: pattern id, sprite: sprite id
//!   A <fps>                       sprite: frame-rate override, 0..30
//!   C <name> <raw…>               pat: control override, raw 16.16 ints
//!   P <mode>                      pat: projection override
//!   R <pct> <pos>:<rrggbb> …      pat: colour ramp
//!   T <source> <arg…>             text: lit <utf8…> | clock <fmt> | slot <n>
//!   F <font> <rrggbb> <align> <scroll> <speed>
//!   K <rrggbb>                    color: the wash colour
//!   X clip abs                    any: `clip` confined to the coverage
//!                                 below, `abs` pinned to grid pixels
//!   M <ms> <x> <y> <opacity> <ease> [<frame>]
//!                                 any: one keyframe of the layer's motion
//! ```
//!
//! [`serialize`] emits only the lines that differ from the defaults, so a
//! parse/serialize round trip is a fixed point.

use alloc::string::String;
use alloc::vec::Vec;

use crate::jsonview::{push_escaped, push_i32, push_piece, push_u32, Sink};
use crate::projection::ProjectionMode;
use crate::text::{ClockFmt, Font};

/// Longest scene name, in UTF-8 bytes.
pub const MAX_NAME: usize = 64;
/// Longest per-layer display name, in UTF-8 bytes.
pub const MAX_LAYER_NAME: usize = 32;
/// Most colour-ramp stops a scene layer may carry — the output palette's
/// cap, since the cooked table is the same 256-entry LUT.
pub const MAX_RAMP_STOPS: usize = crate::outpipe::MAX_OUTPUT_PALETTE_STOPS;
/// Most keyframes (`M` lines) one layer may carry — what bounds a keyed
/// layer's share of the 3,840 B scene blob.
pub const MAX_KEYS: usize = 16;
/// The latest a keyframe may sit, in ms from the scene's start: one hour.
/// The last key's time is the track's loop period.
pub const MAX_KEY_MS: u32 = 3_600_000;
/// The largest design size a `U` line may declare, per axis (Gitea #997).
pub const MAX_SPACE: u16 = 4096;

// ---- types ----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerKind {
    Pattern,
    Text,
    Sprite,
    Color,
}

impl LayerKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            LayerKind::Pattern => "pat",
            LayerKind::Text => "text",
            LayerKind::Sprite => "sprite",
            LayerKind::Color => "color",
        }
    }
    fn from_wire(s: &str) -> Option<LayerKind> {
        match s {
            "pat" => Some(LayerKind::Pattern),
            "text" => Some(LayerKind::Text),
            "sprite" => Some(LayerKind::Sprite),
            "color" => Some(LayerKind::Color),
            _ => None,
        }
    }
    /// Display name when the record carries no `N` line. A pattern layer's
    /// is the empty string: only the host knows the pattern's name.
    pub const fn default_name(self) -> &'static str {
        match self {
            LayerKind::Pattern => "",
            LayerKind::Text => "Text",
            LayerKind::Sprite => "Sprite",
            LayerKind::Color => "Color",
        }
    }
}

/// How a layer is mixed with everything beneath it. Formulas in
/// [`crate::compose`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Blend {
    #[default]
    Normal,
    Add,
    Lighten,
    Multiply,
    Mask,
}

impl Blend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Blend::Normal => "normal",
            Blend::Add => "add",
            Blend::Lighten => "lighten",
            Blend::Multiply => "multiply",
            Blend::Mask => "mask",
        }
    }
    fn from_wire(s: &str) -> Option<Blend> {
        match s {
            "normal" => Some(Blend::Normal),
            "add" => Some(Blend::Add),
            "lighten" => Some(Blend::Lighten),
            "multiply" => Some(Blend::Multiply),
            "mask" => Some(Blend::Mask),
            _ => None,
        }
    }
}

/// Which source pixels are transparent.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Key {
    #[default]
    None,
    /// Pure black is transparent (`blit` mode 3).
    Black,
    /// Alpha = the source pixel's luma.
    Luma,
}

impl Key {
    pub const fn as_str(self) -> &'static str {
        match self {
            Key::None => "none",
            Key::Black => "black",
            Key::Luma => "luma",
        }
    }
    fn from_wire(s: &str) -> Option<Key> {
        match s {
            "none" => Some(Key::None),
            "black" => Some(Key::Black),
            "luma" => Some(Key::Luma),
            _ => None,
        }
    }
}

/// How the layer's content fills its box.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Fit {
    #[default]
    Fill,
    Contain,
    Tile,
}

impl Fit {
    pub const fn as_str(self) -> &'static str {
        match self {
            Fit::Fill => "fill",
            Fit::Contain => "contain",
            Fit::Tile => "tile",
        }
    }
    fn from_wire(s: &str) -> Option<Fit> {
        match s {
            "fill" => Some(Fit::Fill),
            "contain" => Some(Fit::Contain),
            "tile" => Some(Fit::Tile),
            _ => None,
        }
    }
}

/// The layer's box in layout pixels. `w` or `h` = 0 means "the whole
/// layout" on that axis.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i16,
    pub y: i16,
    pub w: u16,
    pub h: u16,
}

/// Everything about a layer that the compositor needs and no layer type
/// owns privately.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LayerStyle {
    pub rect: Rect,
    pub blend: Blend,
    /// 0..=100.
    pub opacity: u8,
    pub key: Key,
    pub fit: Fit,
    pub visible: bool,
    pub flipx: bool,
    pub flipy: bool,
    pub rot180: bool,
}

impl Default for LayerStyle {
    fn default() -> Self {
        LayerStyle {
            rect: Rect::default(),
            blend: Blend::Normal,
            opacity: 100,
            key: Key::None,
            fit: Fit::Fill,
            visible: true,
            flipx: false,
            flipy: false,
            rot180: false,
        }
    }
}

/// `flags` bit positions on the `L` line.
pub const FLAG_VISIBLE: u32 = 1;
pub const FLAG_FLIPX: u32 = 2;
pub const FLAG_FLIPY: u32 = 4;
pub const FLAG_ROT180: u32 = 8;

impl LayerStyle {
    pub fn flags(&self) -> u32 {
        (if self.visible { FLAG_VISIBLE } else { 0 })
            | (if self.flipx { FLAG_FLIPX } else { 0 })
            | (if self.flipy { FLAG_FLIPY } else { 0 })
            | (if self.rot180 { FLAG_ROT180 } else { 0 })
    }
    fn set_flags(&mut self, f: u32) {
        self.visible = f & FLAG_VISIBLE != 0;
        self.flipx = f & FLAG_FLIPX != 0;
        self.flipy = f & FLAG_FLIPY != 0;
        self.rot180 = f & FLAG_ROT180 != 0;
    }
}

/// Where a text layer's string comes from. `Clock` and `Slot` are resolved
/// by the HOST every frame and pushed in with
/// [`crate::compose::Compositor::set_text`] — the compositor never reads a
/// wall clock or the slot table itself.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TextSource {
    Lit(String),
    Clock(ClockFmt),
    Slot(u8),
}

impl Default for TextSource {
    fn default() -> Self {
        TextSource::Lit(String::new())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Align {
    pub const fn as_str(self) -> &'static str {
        match self {
            Align::Left => "l",
            Align::Center => "c",
            Align::Right => "r",
        }
    }
    fn from_wire(s: &str) -> Option<Align> {
        match s {
            "l" => Some(Align::Left),
            "c" => Some(Align::Center),
            "r" => Some(Align::Right),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scroll {
    #[default]
    None,
    Left,
    Right,
    Up,
    Down,
    Bounce,
}

impl Scroll {
    pub const fn as_str(self) -> &'static str {
        match self {
            Scroll::None => "none",
            Scroll::Left => "left",
            Scroll::Right => "right",
            Scroll::Up => "up",
            Scroll::Down => "down",
            Scroll::Bounce => "bounce",
        }
    }
    fn from_wire(s: &str) -> Option<Scroll> {
        match s {
            "none" => Some(Scroll::None),
            "left" => Some(Scroll::Left),
            "right" => Some(Scroll::Right),
            "up" => Some(Scroll::Up),
            "down" => Some(Scroll::Down),
            "bounce" => Some(Scroll::Bounce),
            _ => None,
        }
    }
    /// True for the two vertical directions — [`Scroll::Bounce`] is
    /// horizontal.
    pub const fn vertical(self) -> bool {
        matches!(self, Scroll::Up | Scroll::Down)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TextLayer {
    pub source: TextSource,
    pub font: Font,
    pub color: [u8; 3],
    pub align: Align,
    pub scroll: Scroll,
    /// Scroll rate in pixels per second.
    pub speed: u16,
}

impl Default for TextLayer {
    fn default() -> Self {
        TextLayer {
            source: TextSource::default(),
            font: Font::Regular,
            color: [255, 255, 255],
            align: Align::Left,
            scroll: Scroll::None,
            speed: 0,
        }
    }
}

/// A per-layer colour ramp — the device output-palette stage, applied to
/// this layer's frame alone before it is composited.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ramp {
    /// Blend amount, 0..=100 percent.
    pub pct: u8,
    /// `(position 0..=255, rgb)`, ascending by position, at least two.
    pub stops: Vec<(u8, [u8; 3])>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PatternLayer {
    /// Store pattern id (8 hex), empty when the record carried no `I`.
    pub id: String,
    /// Control overrides, values as raw 16.16 ints — the playlist's `C`.
    pub controls: Vec<(String, Vec<i32>)>,
    /// Projection override as [`ProjectionMode::as_u8`].
    pub proj: Option<u8>,
    pub ramp: Option<Ramp>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LayerBody {
    Pattern(PatternLayer),
    Text(TextLayer),
    Sprite {
        /// Sprite-store id (8 hex), empty when the record carried no `I`.
        id: String,
        /// Per-LAYER frame-rate override, 0..=[`crate::sprite::SPRITE_MAX_FPS`]
        /// — the `A` line. `None` plays the record's own
        /// [`crate::sprite::SpriteView::fps`]; `Some(0)` holds frame 0.
        fps: Option<u8>,
    },
    Color([u8; 3]),
}

impl LayerBody {
    pub fn kind(&self) -> LayerKind {
        match self {
            LayerBody::Pattern(_) => LayerKind::Pattern,
            LayerBody::Text(_) => LayerKind::Text,
            LayerBody::Sprite { .. } => LayerKind::Sprite,
            LayerBody::Color(_) => LayerKind::Color,
        }
    }
}

/// The curve a keyframe track follows on the segment that ENDS at a key.
/// Every curve but `linear` and `step` is a [`crate::ease`] kernel — the
/// same body the pattern language's `ease*` builtins run.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Ease {
    #[default]
    Linear,
    /// Holds the previous key's values and jumps on arrival.
    Step,
    /// Cubic in (`easeInCubic`).
    In,
    /// Cubic out (`easeOutCubic`).
    Out,
    /// Cubic in-out (`easeInOutCubic`).
    InOut,
    /// `easeInOutSine`.
    Sine,
    /// `easeOutBack` — overshoots and settles.
    Back,
    /// `easeOutBounce`.
    Bounce,
    /// `easeOutElastic`.
    Elastic,
}

impl Ease {
    pub const ALL: [Ease; 9] = [
        Ease::Linear,
        Ease::Step,
        Ease::In,
        Ease::Out,
        Ease::InOut,
        Ease::Sine,
        Ease::Back,
        Ease::Bounce,
        Ease::Elastic,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Ease::Linear => "linear",
            Ease::Step => "step",
            Ease::In => "in",
            Ease::Out => "out",
            Ease::InOut => "inout",
            Ease::Sine => "sine",
            Ease::Back => "back",
            Ease::Bounce => "bounce",
            Ease::Elastic => "elastic",
        }
    }
    /// The eased fraction for a segment `u` (0..=1, 16.16) of the way
    /// through. Back and elastic overshoot 0..1 on purpose.
    pub fn apply(self, u: crate::fixed::Fx) -> crate::fixed::Fx {
        use crate::ease;
        match self {
            Ease::Linear => u,
            Ease::Step => {
                if u >= crate::fixed::Fx::ONE {
                    u
                } else {
                    crate::fixed::Fx::ZERO
                }
            }
            Ease::In => ease::in_cubic(u),
            Ease::Out => ease::out_cubic(u),
            Ease::InOut => ease::in_out_cubic(u),
            Ease::Sine => ease::in_out_sine(u),
            Ease::Back => ease::out_back(u),
            Ease::Bounce => ease::out_bounce(u),
            Ease::Elastic => ease::out_elastic(u),
        }
    }
}

/// One `M` line: where the layer is at `ms` from the scene's start. `None`
/// in `x`/`y`/`opacity` is the wire's `=` — the `L` line's own value.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Keyframe {
    pub ms: u32,
    pub x: Option<i16>,
    pub y: Option<i16>,
    /// 0..=100.
    pub opacity: Option<u8>,
    /// The curve of the segment that ends at this key.
    pub ease: Ease,
    /// A sprite frame to hold from this key until the next one; `None`
    /// leaves the sprite's own frame clock running. Ignored on any other
    /// layer kind.
    pub frame: Option<u8>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Layer {
    pub name: String,
    pub style: LayerStyle,
    pub body: LayerBody,
    /// The layer's keyframe track (`M` lines), ascending by `ms`, at most
    /// [`MAX_KEYS`]. Empty = the layer stands where its `L` line puts it.
    pub keys: Vec<Keyframe>,
    /// `X clip`: this layer's α is multiplied by the coverage of the layer
    /// below it (docs/spec/scenes.md §2 "Clip layers").
    pub clip: bool,
    /// `X abs`: in a scene with a design size ([`Scene::space`]) this layer
    /// keeps grid pixels and 1:1 content — it is not mapped (Gitea #997).
    /// Meaningless, and harmless, in a scene without one.
    pub abs: bool,
}

impl Layer {
    pub fn kind(&self) -> LayerKind {
        self.body.kind()
    }
    /// The store PATTERN id a `pat` layer names, if any.
    ///
    /// A `sprite` layer's `I` line carries a SPRITE id (a different store
    /// and a different id mask since Gitea #740) — [`Layer::sprite_id`].
    /// The two used to share this accessor, which is exactly how a sprite
    /// id could reach a pattern lookup.
    pub fn pattern_id(&self) -> Option<&str> {
        match &self.body {
            LayerBody::Pattern(p) if !p.id.is_empty() => Some(&p.id),
            _ => None,
        }
    }

    /// The sprite-store id a `sprite` layer names, if any.
    pub fn sprite_id(&self) -> Option<&str> {
        match &self.body {
            LayerBody::Sprite { id, .. } if !id.is_empty() => Some(id),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Scene {
    /// 8 lowercase hex, or empty when the record said `S -` ("assign one").
    pub id: String,
    pub name: String,
    /// The `U <w> <h>` line: the size the scene was designed at, each axis
    /// 1..=[`MAX_SPACE`]. `Some` makes every box and keyframe position a
    /// DESIGN unit that the compositor maps onto the real grid
    /// (docs/spec/scenes.md §2 "Design size"); `None` is layout pixels.
    pub space: Option<(u16, u16)>,
    /// Bottom → top; `layers[0]` is the base.
    pub layers: Vec<Layer>,
}

/// How many `pat` layers a scene holds — the number that must fit
/// `caps.layers`, since only those cost a resident engine.
pub fn pattern_layers(s: &Scene) -> usize {
    s.layers
        .iter()
        .filter(|l| l.kind() == LayerKind::Pattern)
        .count()
}

// ---- parsing ----

/// True for a well-formed record id: exactly 8 lowercase hex digits, the
/// same shape the pattern store assigns.
pub fn valid_id(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `scene: line N: <what>` — assembled without `format!`, which costs the
/// firmware image a full `Arguments` plumbing per call site.
fn err(line: usize, what: &str) -> String {
    let mut s = String::with_capacity(what.len() + 20);
    s.push_str("scene: line ");
    push_u32(&mut s, line as u32);
    s.push_str(": ");
    s.push_str(what);
    s
}

/// `scene: line N: <what> "<tok>"`.
fn err_tok(line: usize, what: &str, tok: &str) -> String {
    let mut s = err(line, what);
    s.push_str(" \"");
    s.push_str(tok);
    s.push('"');
    s
}

/// The remainder of `line` after `skip` whitespace-separated tokens, with
/// the separating whitespace removed. `""` when the line is shorter.
fn rest(line: &str, skip: usize) -> &str {
    let mut s = line;
    for _ in 0..skip {
        s = s.trim_start();
        match s.find(char::is_whitespace) {
            Some(i) => s = &s[i..],
            None => return "",
        }
    }
    s.trim_start()
}

/// `rrggbb`, either case on the way in.
fn parse_rgb(s: &str) -> Option<[u8; 3]> {
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let h = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([h(0)?, h(2)?, h(4)?])
}

fn push_rgb(out: &mut dyn Sink, c: [u8; 3]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 6];
    for (i, v) in c.iter().enumerate() {
        buf[i * 2] = HEX[(v >> 4) as usize];
        buf[i * 2 + 1] = HEX[(v & 15) as usize];
    }
    // ASCII hex by construction; the fallback keeps this `unsafe`-free
    push_piece(out, core::str::from_utf8(&buf).unwrap_or(""));
}

/// Parse one scene block. Line numbers in error messages are 1-based
/// within `block`.
pub fn parse(block: &str) -> Result<Scene, String> {
    parse_at(block, 1)
}

/// Parse a whole `SCENES_KEY` blob: scene blocks back to back, each
/// starting at its own `S` line. Error line numbers are blob-global.
pub fn parse_all(blob: &str) -> Result<Vec<Scene>, String> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut buf = String::new();
    for (i, line) in blob.lines().enumerate() {
        let is_s = line.trim_start().starts_with("S ") || line.trim() == "S";
        if is_s {
            if let Some(at) = start.take() {
                out.push(parse_at(&buf, at)?);
            }
            buf.clear();
            start = Some(i + 1);
        }
        if start.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
        // lines before the first S are ignored, like any unknown line
    }
    if let Some(at) = start {
        out.push(parse_at(&buf, at)?);
    }
    Ok(out)
}

fn parse_at(block: &str, first_line: usize) -> Result<Scene, String> {
    let mut scene = Scene::default();
    let mut seen_s = false;
    for (i, raw) in block.lines().enumerate() {
        let line = raw.trim_end_matches('\r');
        let n = first_line + i;
        let tag = line.split_whitespace().next().unwrap_or("");
        match tag {
            "S" => {
                if seen_s {
                    return Err(err(n, "a second S line in one scene block"));
                }
                seen_s = true;
                let mut it = line.split_whitespace();
                it.next();
                let id = it.next().unwrap_or("");
                if id != "-" && !valid_id(id) {
                    return Err(err_tok(n, "bad scene id", id));
                }
                scene.id = if id == "-" { String::new() } else { id.into() };
                let name = rest(line, 2);
                if name.len() > MAX_NAME {
                    return Err(err(n, "scene name is over 64 bytes"));
                }
                scene.name = name.into();
            }
            "L" => {
                if !seen_s {
                    return Err(err(n, "an L line before the S line"));
                }
                scene.layers.push(parse_layer(line, n)?);
            }
            // The design size (Gitea #997): scene-level, so it goes between
            // the `S` and the first `L`. An older host reads it as a binding
            // line before any layer — ignored — and draws the numbers 1:1.
            "U" if seen_s && scene.layers.is_empty() => {
                let mut it = line.split_whitespace().skip(1);
                let mut dim = || -> Result<u16, String> {
                    let tok = it.next().unwrap_or("");
                    match tok.parse::<u16>() {
                        Ok(v) if (1..=MAX_SPACE).contains(&v) => Ok(v),
                        _ => Err(err_tok(n, "bad U field", tok)),
                    }
                };
                scene.space = Some((dim()?, dim()?));
            }
            "U" if seen_s => return Err(err(n, "a U line after the first L line")),
            "" => {}
            _ => {
                if !seen_s {
                    return Err(err(n, "a binding line before the S line"));
                }
                // Binding lines attach to the most recent L. One that
                // arrives before any layer, or that does not apply to the
                // layer's type, reads as an unknown line: ignored, so a
                // newer console's extra lines never brick an older host.
                if let Some(layer) = scene.layers.last_mut() {
                    parse_binding(layer, tag, line, n)?;
                }
            }
        }
    }
    if !seen_s {
        return Err(err(first_line, "expected an S line"));
    }
    Ok(scene)
}

fn parse_int(tok: &str, line: usize, what: &str) -> Result<i32, String> {
    tok.parse::<i32>().map_err(|_| err_tok(line, what, tok))
}

fn parse_layer(line: &str, n: usize) -> Result<Layer, String> {
    let mut it = line.split_whitespace();
    it.next();
    let mut next = |what: &str| -> Result<&str, String> {
        it.next().ok_or_else(|| {
            let mut s = err(n, "L needs 10 fields, missing ");
            s.push_str(what);
            s
        })
    };
    let kind_tok = next("type")?;
    let kind = LayerKind::from_wire(kind_tok)
        .ok_or_else(|| err_tok(n, "unknown layer type", kind_tok))?;
    let x = parse_int(next("x")?, n, "x is not an integer")?;
    let y = parse_int(next("y")?, n, "y is not an integer")?;
    let w = parse_int(next("w")?, n, "w is not an integer")?;
    let h = parse_int(next("h")?, n, "h is not an integer")?;
    let blend_tok = next("blend")?;
    let blend = Blend::from_wire(blend_tok).ok_or_else(|| err_tok(n, "unknown blend", blend_tok))?;
    let opacity_tok = next("opacity")?;
    let opacity = parse_int(opacity_tok, n, "opacity is not an integer")?;
    if !(0..=100).contains(&opacity) {
        return Err(err_tok(n, "opacity must be 0..100", opacity_tok));
    }
    let key_tok = next("key")?;
    let key = Key::from_wire(key_tok).ok_or_else(|| err_tok(n, "unknown key", key_tok))?;
    let fit_tok = next("fit")?;
    let fit = Fit::from_wire(fit_tok).ok_or_else(|| err_tok(n, "unknown fit", fit_tok))?;
    let flags_tok = next("flags")?;
    let flags = parse_int(flags_tok, n, "flags is not an integer")?;
    if !(0..=15).contains(&flags) {
        return Err(err_tok(n, "flags must be 0..15", flags_tok));
    }
    if !(i16::MIN as i32..=i16::MAX as i32).contains(&x)
        || !(i16::MIN as i32..=i16::MAX as i32).contains(&y)
    {
        return Err(err(n, "x and y must fit a signed 16-bit integer"));
    }
    if !(0..=u16::MAX as i32).contains(&w) || !(0..=u16::MAX as i32).contains(&h) {
        return Err(err(n, "w and h must be 0..65535"));
    }
    let mut style = LayerStyle {
        rect: Rect {
            x: x as i16,
            y: y as i16,
            w: w as u16,
            h: h as u16,
        },
        blend,
        opacity: opacity as u8,
        key,
        fit,
        ..LayerStyle::default()
    };
    style.set_flags(flags as u32);
    let body = match kind {
        LayerKind::Pattern => LayerBody::Pattern(PatternLayer::default()),
        LayerKind::Text => LayerBody::Text(TextLayer::default()),
        LayerKind::Sprite => LayerBody::Sprite {
            id: String::new(),
            fps: None,
        },
        LayerKind::Color => LayerBody::Color([0, 0, 0]),
    };
    Ok(Layer {
        name: kind.default_name().into(),
        style,
        body,
        keys: Vec::new(),
        clip: false,
        abs: false,
    })
}

fn parse_binding(layer: &mut Layer, tag: &str, line: &str, n: usize) -> Result<(), String> {
    let mut it = line.split_whitespace();
    it.next();
    match tag {
        "N" => {
            let name = rest(line, 1);
            if name.len() > MAX_LAYER_NAME {
                return Err(err(n, "layer name is over 32 bytes"));
            }
            layer.name = name.into();
        }
        "I" => {
            let id = it.next().unwrap_or("");
            if !valid_id(id) {
                return Err(err_tok(n, "bad pattern id", id));
            }
            match &mut layer.body {
                LayerBody::Pattern(p) => p.id = id.into(),
                LayerBody::Sprite { id: s, .. } => *s = id.into(),
                _ => {}
            }
        }
        // The sprite layer's frame-rate override. Unlike the other bindings
        // this one REFUSES a layer it does not apply to rather than ignoring
        // it: `A` on a `pat`/`text`/`color` layer can only be a console bug
        // or a hand-written record, and silently dropping a rate the user
        // typed is worse than naming it (Gitea #740 follow-up).
        "A" => {
            let tok = it.next().unwrap_or("");
            let fps = parse_int(tok, n, "sprite fps is not an integer")?;
            if !(0..=crate::sprite::SPRITE_MAX_FPS as i32).contains(&fps) {
                return Err(err_tok(n, "sprite fps must be 0..30", tok));
            }
            match &mut layer.body {
                LayerBody::Sprite { fps: f, .. } => *f = Some(fps as u8),
                _ => return Err(err(n, "A on a non-sprite layer")),
            }
        }
        "C" => {
            if let (LayerBody::Pattern(p), Some(name)) = (&mut layer.body, it.next()) {
                let mut raw = Vec::new();
                for tok in it {
                    raw.push(parse_int(tok, n, "control values are raw 16.16 integers")?);
                }
                p.controls.push((name.into(), raw));
            }
        }
        "P" => {
            let tok = it.next().unwrap_or("");
            let mode: ProjectionMode = tok
                .parse()
                .map_err(|_| err_tok(n, "unknown projection", tok))?;
            if let LayerBody::Pattern(p) = &mut layer.body {
                p.proj = Some(mode.as_u8());
            }
        }
        "R" => {
            let pct_tok = it.next().unwrap_or("");
            let pct = parse_int(pct_tok, n, "ramp amount is not an integer")?;
            if !(0..=100).contains(&pct) {
                return Err(err_tok(n, "ramp amount must be 0..100", pct_tok));
            }
            let mut stops: Vec<(u8, [u8; 3])> = Vec::new();
            for tok in it {
                let (pos, rgb) = tok
                    .split_once(':')
                    .ok_or_else(|| err_tok(n, "a ramp stop is <pos>:<rrggbb>", tok))?;
                let pos = parse_int(pos, n, "a ramp stop position is 0..255")?;
                if !(0..=255).contains(&pos) {
                    return Err(err_tok(n, "a ramp stop position is 0..255", tok));
                }
                let rgb = parse_rgb(rgb).ok_or_else(|| err_tok(n, "a ramp stop colour is rrggbb", tok))?;
                if stops.len() == MAX_RAMP_STOPS {
                    return Err(err(n, "too many ramp stops (max 32)"));
                }
                stops.push((pos as u8, rgb));
            }
            if stops.len() < 2 {
                return Err(err(n, "a ramp needs at least two stops"));
            }
            if stops.windows(2).any(|w| w[0].0 > w[1].0) {
                return Err(err(n, "ramp stops must ascend by position"));
            }
            if let LayerBody::Pattern(p) = &mut layer.body {
                p.ramp = Some(Ramp {
                    pct: pct as u8,
                    stops,
                });
            }
        }
        "T" => {
            let src = it.next().unwrap_or("");
            let source = match src {
                "lit" => {
                    let t = rest(line, 2);
                    if t.len() > MAX_NAME {
                        return Err(err(n, "text is over 64 bytes"));
                    }
                    TextSource::Lit(t.into())
                }
                "clock" => {
                    let f = it.next().unwrap_or("");
                    TextSource::Clock(
                        ClockFmt::from_wire(f)
                            .ok_or_else(|| err_tok(n, "unknown clock format", f))?,
                    )
                }
                "slot" => {
                    let tok = it.next().unwrap_or("");
                    let v = parse_int(tok, n, "a text slot is 0..7")?;
                    if !(0..crate::text::SLOTS as i32).contains(&v) {
                        return Err(err_tok(n, "a text slot is 0..7", tok));
                    }
                    TextSource::Slot(v as u8)
                }
                _ => return Err(err_tok(n, "unknown text source", src)),
            };
            if let LayerBody::Text(t) = &mut layer.body {
                t.source = source;
            }
        }
        "F" => {
            let font_tok = it.next().unwrap_or("");
            let font =
                Font::from_wire(font_tok).ok_or_else(|| err_tok(n, "unknown font", font_tok))?;
            let color_tok = it.next().unwrap_or("");
            let color =
                parse_rgb(color_tok).ok_or_else(|| err_tok(n, "text colour is rrggbb", color_tok))?;
            let align_tok = it.next().unwrap_or("");
            let align =
                Align::from_wire(align_tok).ok_or_else(|| err_tok(n, "unknown align", align_tok))?;
            let scroll_tok = it.next().unwrap_or("");
            let scroll = Scroll::from_wire(scroll_tok)
                .ok_or_else(|| err_tok(n, "unknown scroll", scroll_tok))?;
            let speed_tok = it.next().unwrap_or("");
            let speed = parse_int(speed_tok, n, "scroll speed is not an integer")?;
            if !(0..=u16::MAX as i32).contains(&speed) {
                return Err(err_tok(n, "scroll speed must be 0..65535", speed_tok));
            }
            if let LayerBody::Text(t) = &mut layer.body {
                t.font = font;
                t.color = color;
                t.align = align;
                t.scroll = scroll;
                t.speed = speed as u16;
            }
        }
        "K" => {
            let tok = it.next().unwrap_or("");
            let rgb = parse_rgb(tok).ok_or_else(|| err_tok(n, "wash colour is rrggbb", tok))?;
            if let LayerBody::Color(c) = &mut layer.body {
                *c = rgb;
            }
        }
        // `X <flag…>`: per-layer compositing flags that do not fit the `L`
        // line's 0..15 `flags` (an old host REJECTS a bigger number, but
        // ignores an unknown line). Unknown words are a newer console's.
        "X" => {
            for w in it {
                match w {
                    "clip" => layer.clip = true,
                    "abs" => layer.abs = true,
                    _ => {}
                }
            }
        }
        "M" => {
            let key = parse_key(it, n)?;
            if layer.keys.len() == MAX_KEYS {
                return Err(err(n, "too many keyframes (max 16)"));
            }
            if layer.keys.last().is_some_and(|k| k.ms > key.ms) {
                return Err(err(n, "keyframe times must not go backwards"));
            }
            layer.keys.push(key);
        }
        // Anything else is a line from a newer console: ignored.
        _ => {}
    }
    Ok(())
}

/// The fields of one `M` line after its tag:
/// `<ms> <x> <y> <opacity> <ease> [<frame>]`, `=` for an unchanged
/// x / y / opacity. Trailing tokens past `frame` are a newer console's and
/// are ignored, like every binding line's.
///
/// One table-driven loop and ONE message naming the offending token: every
/// byte here ships in the firmware image AND the console's wasm, whose
/// asset partition has ~13 KB of headroom.
fn parse_key<'a>(it: impl Iterator<Item = &'a str>, n: usize) -> Result<Keyframe, String> {
    const I16: (i32, i32) = (i16::MIN as i32, i16::MAX as i32);
    // ms, x, y, opacity, ease (an index into `Ease::ALL`), frame
    const LIM: [(i32, i32); 6] = [(0, MAX_KEY_MS as i32), I16, I16, (0, 100), (0, 8), (0, 255)];
    let mut f: [Option<i32>; 6] = [None; 6];
    let mut seen = 0;
    for (i, tok) in it.take(6).enumerate() {
        seen = i + 1;
        if tok == "=" && (1..=3).contains(&i) {
            continue;
        }
        let v = if i == 4 {
            Ease::ALL.iter().position(|e| e.as_str() == tok).map_or(-1, |p| p as i32)
        } else {
            tok.parse::<i32>().unwrap_or(i32::MIN)
        };
        if v < LIM[i].0 || v > LIM[i].1 {
            return Err(err_tok(n, "bad M field", tok));
        }
        f[i] = Some(v);
    }
    if seen < 5 {
        return Err(err(n, "M needs <ms> <x> <y> <opacity> <ease> [<frame>]"));
    }
    Ok(Keyframe {
        ms: f[0].unwrap_or(0) as u32,
        x: f[1].map(|v| v as i16),
        y: f[2].map(|v| v as i16),
        opacity: f[3].map(|v| v as u8),
        ease: Ease::ALL[f[4].unwrap_or(0) as usize],
        frame: f[5].map(|v| v as u8),
    })
}

// ---- serialization ----

/// Emit the wire block for `s`, defaults omitted, so `parse` ∘ `serialize`
/// is the identity on the record and `serialize` ∘ `parse` is a fixed
/// point on the text.
pub fn serialize(s: &Scene, out: &mut String) {
    out.push_str("S ");
    out.push_str(if s.id.is_empty() { "-" } else { &s.id });
    if !s.name.is_empty() {
        out.push(' ');
        out.push_str(&s.name);
    }
    out.push('\n');
    if let Some((w, h)) = s.space {
        out.push_str("U ");
        push_u32(out, w as u32);
        out.push(' ');
        push_u32(out, h as u32);
        out.push('\n');
    }
    for l in &s.layers {
        let st = &l.style;
        out.push_str("L ");
        out.push_str(l.kind().as_str());
        out.push(' ');
        push_i32(out, st.rect.x as i32);
        out.push(' ');
        push_i32(out, st.rect.y as i32);
        out.push(' ');
        push_u32(out, st.rect.w as u32);
        out.push(' ');
        push_u32(out, st.rect.h as u32);
        out.push(' ');
        out.push_str(st.blend.as_str());
        out.push(' ');
        push_u32(out, st.opacity as u32);
        out.push(' ');
        out.push_str(st.key.as_str());
        out.push(' ');
        out.push_str(st.fit.as_str());
        out.push(' ');
        push_u32(out, st.flags());
        out.push('\n');
        if l.name != l.kind().default_name() {
            out.push_str("N ");
            out.push_str(&l.name);
            out.push('\n');
        }
        match &l.body {
            LayerBody::Pattern(p) => {
                if !p.id.is_empty() {
                    out.push_str("I ");
                    out.push_str(&p.id);
                    out.push('\n');
                }
                for (name, raw) in &p.controls {
                    out.push_str("C ");
                    out.push_str(name);
                    for v in raw {
                        out.push(' ');
                        push_i32(out, *v);
                    }
                    out.push('\n');
                }
                if let Some(mode) = p.proj.and_then(ProjectionMode::from_u8) {
                    out.push_str("P ");
                    out.push_str(mode.as_str());
                    out.push('\n');
                }
                if let Some(r) = &p.ramp {
                    out.push_str("R ");
                    push_u32(out, r.pct as u32);
                    for (pos, rgb) in &r.stops {
                        out.push(' ');
                        push_u32(out, *pos as u32);
                        out.push(':');
                        push_rgb(out, *rgb);
                    }
                    out.push('\n');
                }
            }
            LayerBody::Text(t) => {
                match &t.source {
                    TextSource::Lit(v) if v.is_empty() => {}
                    TextSource::Lit(v) => {
                        out.push_str("T lit ");
                        out.push_str(v);
                        out.push('\n');
                    }
                    TextSource::Clock(f) => {
                        out.push_str("T clock ");
                        out.push_str(f.as_str());
                        out.push('\n');
                    }
                    TextSource::Slot(k) => {
                        out.push_str("T slot ");
                        push_u32(out, *k as u32);
                        out.push('\n');
                    }
                }
                let unstyled = TextLayer {
                    source: t.source.clone(),
                    ..TextLayer::default()
                };
                if *t != unstyled {
                    out.push_str("F ");
                    out.push_str(t.font.as_str());
                    out.push(' ');
                    push_rgb(out, t.color);
                    out.push(' ');
                    out.push_str(t.align.as_str());
                    out.push(' ');
                    out.push_str(t.scroll.as_str());
                    out.push(' ');
                    push_u32(out, t.speed as u32);
                    out.push('\n');
                }
            }
            LayerBody::Sprite { id, fps } => {
                if !id.is_empty() {
                    out.push_str("I ");
                    out.push_str(id);
                    out.push('\n');
                }
                if let Some(f) = fps {
                    out.push_str("A ");
                    push_u32(out, *f as u32);
                    out.push('\n');
                }
            }
            LayerBody::Color(c) => {
                if *c != [0, 0, 0] {
                    out.push_str("K ");
                    push_rgb(out, *c);
                    out.push('\n');
                }
            }
        }
        // one `X` line, its words in a fixed order: `clip`, then `abs`
        if l.clip || l.abs {
            out.push('X');
            if l.clip {
                out.push_str(" clip");
            }
            if l.abs {
                out.push_str(" abs");
            }
            out.push('\n');
        }
        for k in &l.keys {
            push_key(out, k);
        }
    }
}

/// `M <ms> <x> <y> <opacity> <ease>[ <frame>]`, `=` for a kept value.
///
/// Out of line and through [`push_piece`] / [`push_num`] only: inlined into
/// `serialize`'s callers, the integer formatting and `String::push_str` are
/// copied per call site, which cost the classic-ESP32 image ~3 KB when this
/// shipped (the OTA slot's 3 % floor is a few hundred bytes away there).
#[inline(never)]
fn push_key(out: &mut dyn Sink, k: &Keyframe) {
    push_piece(out, "M ");
    push_num(out, Some(k.ms as i32));
    for v in [k.x.map(i32::from), k.y.map(i32::from), k.opacity.map(i32::from)] {
        push_piece(out, " ");
        push_num(out, v);
    }
    push_piece(out, " ");
    push_piece(out, k.ease.as_str());
    if let Some(f) = k.frame {
        push_piece(out, " ");
        push_num(out, Some(f as i32));
    }
    push_piece(out, "\n");
}

/// An integer, or `=` for `None` (the wire's "keep the L line's value").
#[inline(never)]
fn push_num(out: &mut dyn Sink, v: Option<i32>) {
    match v {
        Some(v) => push_i32(out, v),
        None => push_piece(out, "="),
    }
}

/// `,"keys":[…]` — out of line for the same reason as [`push_key`].
#[inline(never)]
fn push_keys_json(out: &mut dyn Sink, keys: &[Keyframe]) {
    for (ki, k) in keys.iter().enumerate() {
        push_piece(out, if ki == 0 { ",\"keys\":[{\"ms\":" } else { ",{\"ms\":" });
        push_num(out, Some(k.ms as i32));
        for (name, v) in [
            (",\"x\":", k.x.map(i32::from)),
            (",\"y\":", k.y.map(i32::from)),
            (",\"opacity\":", k.opacity.map(i32::from)),
        ] {
            if v.is_some() {
                push_piece(out, name);
                push_num(out, v);
            }
        }
        push_piece(out, ",\"ease\":\"");
        push_piece(out, k.ease.as_str());
        push_piece(out, "\"");
        if let Some(f) = k.frame {
            push_piece(out, ",\"frame\":");
            push_num(out, Some(f as i32));
        }
        push_piece(out, "}");
    }
    push_piece(out, "]");
}

// ---- JSON (the `/api/scenes` shape) ----

/// `"key":"escaped value"`.
fn push_str_field(out: &mut dyn Sink, key: &str, v: &str) {
    push_piece(out, "\"");
    push_piece(out, key);
    push_piece(out, "\":\"");
    push_escaped(out, v);
    push_piece(out, "\"");
}

/// Widest `Fx::dec_str` output: sign, five integer digits, the point and
/// sixteen fraction digits (`-0.0000152587890625`).
const FX_DEC_MAX: usize = 23;

/// Every FIXED byte [`push_json`] writes for one layer object: the keys, the
/// widest spelling of each enum, the widest decimal each number can take,
/// and the comma that separates the layer from the one before it. The layer
/// name and the body are the variable parts and are added on top.
const LAYER_FIXED: usize = 240;

/// `,"keys":[]` around a layer's keyframes, and the widest one of them:
/// `{"ms":3600000,"x":-32768,"y":-32768,"opacity":100,"ease":"elastic","frame":255}`
/// plus its separating comma.
const KEYS_FIXED: usize = 10;
const KEY_JSON_MAX: usize = 84;

/// The scene object around its layers — `{"id":"…","name":"…","layers":[]}`
/// with both strings empty, plus the widest `,"space":[4096,4096]`.
const SCENE_FIXED: usize = 60;

/// An UPPER bound on the bytes [`push_json`] appends for `s`, computed
/// without building anything.
///
/// This is what lets `GET /api/scenes` be ONE fallible reservation instead
/// of a `String` that doubles its way to a few KB — which, on a board
/// holding a scene at its pattern-layer cap, is an allocator panic on a
/// read-only poll (Gitea #728). The host test `push_json_never_outgrows_its_bound`
/// is what holds it: raise the constants if it ever fails, never relax the
/// test.
pub fn json_bound(s: &Scene) -> usize {
    let esc = crate::jsonview::json_escape_len;
    let mut n = SCENE_FIXED + esc(&s.id) + esc(&s.name);
    for l in &s.layers {
        // `,"clip":true` and `,"abs":true` included
        n += LAYER_FIXED + 23 + esc(&l.name);
        if !l.keys.is_empty() {
            n += KEYS_FIXED + l.keys.len() * KEY_JSON_MAX;
        }
        n += match &l.body {
            LayerBody::Pattern(p) => {
                // `,"pat":{"id":"…","controls":{…}}` plus the optional
                // projection and ramp
                let mut b = 40 + esc(&p.id);
                for (name, raw) in &p.controls {
                    b += esc(name) + 6 + raw.len() * (FX_DEC_MAX + 1);
                }
                if p.proj.is_some() {
                    b += 20;
                }
                if let Some(r) = &p.ramp {
                    b += 32 + r.stops.len() * 16;
                }
                b
            }
            // the widest `source` arm is `clock`; a `lit` one adds its text
            LayerBody::Text(t) => {
                132 + match &t.source {
                    TextSource::Lit(v) => esc(v),
                    _ => 0,
                }
            }
            // `,"sprite":{"id":"…"}` plus the optional `,"fps":30`
            LayerBody::Sprite { id, .. } => 40 + esc(id),
            LayerBody::Color(_) => 20,
        };
    }
    n
}

/// One scene as the JSON object `GET /api/scenes` carries. Control values
/// are decimal, like the playlist's.
///
/// Never writes more than [`json_bound`] says, so a caller that reserved
/// that much cannot make this reallocate.
pub fn push_json(s: &Scene, out: &mut dyn Sink) {
    push_piece(out, "{\"id\":\"");
    push_escaped(out, &s.id);
    push_piece(out, "\",");
    push_str_field(out, "name", &s.name);
    // present only when the scene declares a design size (Gitea #997)
    if let Some((w, h)) = s.space {
        push_piece(out, ",\"space\":[");
        push_u32(out, w as u32);
        push_piece(out, ",");
        push_u32(out, h as u32);
        push_piece(out, "]");
    }
    push_piece(out, ",\"layers\":[");
    for (i, l) in s.layers.iter().enumerate() {
        if i > 0 {
            push_piece(out, ",");
        }
        let st = &l.style;
        push_piece(out, "{\"type\":\"");
        push_piece(out, l.kind().as_str());
        push_piece(out, "\",");
        push_str_field(out, "name", &l.name);
        push_piece(out, ",\"x\":");
        push_i32(out, st.rect.x as i32);
        push_piece(out, ",\"y\":");
        push_i32(out, st.rect.y as i32);
        push_piece(out, ",\"w\":");
        push_u32(out, st.rect.w as u32);
        push_piece(out, ",\"h\":");
        push_u32(out, st.rect.h as u32);
        push_piece(out, ",\"blend\":\"");
        push_piece(out, st.blend.as_str());
        push_piece(out, "\",\"opacity\":");
        push_u32(out, st.opacity as u32);
        push_piece(out, ",\"key\":\"");
        push_piece(out, st.key.as_str());
        push_piece(out, "\",\"fit\":\"");
        push_piece(out, st.fit.as_str());
        push_piece(out, "\",\"visible\":");
        push_bool(out, st.visible);
        push_piece(out, ",\"flipx\":");
        push_bool(out, st.flipx);
        push_piece(out, ",\"flipy\":");
        push_bool(out, st.flipy);
        push_piece(out, ",\"rot180\":");
        push_bool(out, st.rot180);
        // present only when set, like the sprite's `fps`
        if l.clip {
            push_piece(out, ",\"clip\":true");
        }
        if l.abs {
            push_piece(out, ",\"abs\":true");
        }
        if !l.keys.is_empty() {
            push_keys_json(out, &l.keys);
        }
        match &l.body {
            LayerBody::Pattern(p) => {
                push_piece(out, ",\"pat\":{");
                push_str_field(out, "id", &p.id);
                push_piece(out, ",\"controls\":{");
                for (ci, (name, raw)) in p.controls.iter().enumerate() {
                    if ci > 0 {
                        push_piece(out, ",");
                    }
                    push_piece(out, "\"");
                    push_escaped(out, name);
                    push_piece(out, "\":[");
                    for (vi, &r) in raw.iter().enumerate() {
                        if vi > 0 {
                            push_piece(out, ",");
                        }
                        // Fx's Display, not f64's — core's float formatter
                        // is ~8 KB of image the firmware must not link.
                        let mut b = [0u8; 24];
                        push_piece(out, crate::fixed::Fx::from_raw(r).dec_str(&mut b));
                    }
                    push_piece(out, "]");
                }
                push_piece(out, "}");
                if let Some(mode) = p.proj.and_then(ProjectionMode::from_u8) {
                    push_piece(out, ",\"proj\":\"");
                    push_piece(out, mode.as_str());
                    push_piece(out, "\"");
                }
                if let Some(r) = &p.ramp {
                    push_piece(out, ",\"ramp\":{\"pct\":");
                    push_u32(out, r.pct as u32);
                    push_piece(out, ",\"stops\":[");
                    for (si, (pos, rgb)) in r.stops.iter().enumerate() {
                        if si > 0 {
                            push_piece(out, ",");
                        }
                        push_piece(out, "[");
                        push_u32(out, *pos as u32);
                        push_piece(out, ",\"");
                        push_rgb(out, *rgb);
                        push_piece(out, "\"]");
                    }
                    push_piece(out, "]");
                    push_piece(out, "}");
                }
                push_piece(out, "}");
            }
            LayerBody::Text(t) => {
                push_piece(out, ",\"text\":{\"source\":\"");
                match &t.source {
                    TextSource::Lit(v) => {
                        push_piece(out, "lit\",");
                        push_str_field(out, "text", v);
                    }
                    TextSource::Clock(f) => {
                        push_piece(out, "clock\",");
                        push_str_field(out, "fmt", f.as_str());
                    }
                    TextSource::Slot(k) => {
                        push_piece(out, "slot\",\"slot\":");
                        push_u32(out, *k as u32);
                    }
                }
                push_piece(out, ",\"font\":\"");
                push_piece(out, t.font.as_str());
                push_piece(out, "\",\"color\":\"");
                push_rgb(out, t.color);
                push_piece(out, "\",\"align\":\"");
                push_piece(out, t.align.as_str());
                push_piece(out, "\",\"scroll\":\"");
                push_piece(out, t.scroll.as_str());
                push_piece(out, "\",\"speed\":");
                push_u32(out, t.speed as u32);
                push_piece(out, "}");
            }
            LayerBody::Sprite { id, fps } => {
                push_piece(out, ",\"sprite\":{");
                push_str_field(out, "id", id);
                // present only when the layer overrides the record's rate
                if let Some(f) = fps {
                    push_piece(out, ",\"fps\":");
                    push_u32(out, *f as u32);
                }
                push_piece(out, "}");
            }
            LayerBody::Color(c) => {
                push_piece(out, ",\"color\":\"");
                push_rgb(out, *c);
                push_piece(out, "\"");
            }
        }
        push_piece(out, "}");
    }
    push_piece(out, "]}");
}

fn push_bool(out: &mut dyn Sink, v: bool) {
    push_piece(out, if v { "true" } else { "false" });
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn round(block: &str) -> String {
        let s = parse(block).expect("parses");
        let mut out = String::new();
        serialize(&s, &mut out);
        out
    }

    #[test]
    fn a_minimal_scene_round_trips() {
        let wire = "S 5eed1c92 Clock wall\nL text 0 0 0 0 normal 100 none fill 1\nT lit HI\n";
        assert_eq!(round(wire), wire);
    }

    #[test]
    fn every_layer_line_round_trips() {
        let wire = concat!(
            "S 00ab12ef My scene\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "K ff8800\n",
            "L pat -3 4 16 8 multiply 60 luma tile 7\n",
            "N Base\n",
            "I 5eed1c9d\n",
            "C speed 32768 100\n",
            "C hue -65536\n",
            "P xy\n",
            "R 80 0:000000 128:ff0000 255:ffffff\n",
            "L sprite 2 2 8 8 add 45 black fill 15\n",
            "I 0123abcd\n",
            "A 6\n",
            "L text 1 1 32 7 lighten 100 none contain 3\n",
            "N Ticker\n",
            "T clock HH:MM:SS\n",
            "F tiny 00ff00 c bounce 12\n",
        );
        assert_eq!(round(wire), wire);
        // and the record itself survives a second pass unchanged
        assert_eq!(parse(wire).unwrap(), parse(&round(wire)).unwrap());
    }

    #[test]
    fn defaults_are_omitted_on_serialize() {
        // every binding line here restates a default, so none is emitted
        let wire = concat!(
            "S 0000000a n\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
            "N Text\n",
            "T lit \n",
            "F regular ffffff l none 0\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "N Color\n",
            "K 000000\n",
        );
        assert_eq!(
            round(wire),
            "S 0000000a n\nL text 0 0 0 0 normal 100 none fill 1\nL color 0 0 0 0 normal 100 none fill 1\n"
        );
    }

    #[test]
    fn a_pattern_layer_keeps_no_default_name() {
        // Only the host knows a pattern's name, so `N` is emitted whenever
        // the record carries one and omitted when it does not.
        let wire = "S 0000000a n\nL pat 0 0 0 0 normal 100 none fill 1\nI 0123abcd\n";
        assert_eq!(round(wire), wire);
        let s = parse(wire).unwrap();
        assert_eq!(s.layers[0].name, "");
        assert_eq!(s.layers[0].pattern_id(), Some("0123abcd"));
        assert_eq!(s.layers[0].sprite_id(), None);
    }

    /// A `sprite` layer's `I` line is a SPRITE-store id, and the two
    /// accessors must not answer for each other's layer kind — that is how
    /// a sprite id could otherwise reach a pattern lookup (Gitea #740).
    #[test]
    fn a_sprite_layers_id_is_not_a_pattern_id() {
        let wire = "S 0000000a n\nL sprite 0 0 0 0 normal 100 none fill 1\nI 5b17e5ef\n";
        assert_eq!(round(wire), wire, "the binding line round-trips unchanged");
        let s = parse(wire).unwrap();
        assert_eq!(s.layers[0].kind(), LayerKind::Sprite);
        assert_eq!(s.layers[0].sprite_id(), Some("5b17e5ef"));
        assert_eq!(s.layers[0].pattern_id(), None);
        // an empty binding answers neither
        let bare = parse("S 0000000a n\nL sprite 0 0 0 0 normal 100 none fill 1\n").unwrap();
        assert_eq!(bare.layers[0].sprite_id(), None);
        assert_eq!(bare.layers[0].pattern_id(), None);
    }

    /// `A <fps>` — the sprite LAYER's frame-rate override. Absent means the
    /// record's own rate, so it must not be invented on serialize.
    #[test]
    fn a_sprite_layer_can_override_the_records_fps() {
        let head = "S 0000000a n\nL sprite 0 0 0 0 normal 100 none fill 1\nI 5b17e5ef\n";
        // absent: the record's own rate, and nothing emitted
        let bare = parse(head).unwrap();
        assert_eq!(
            bare.layers[0].body,
            LayerBody::Sprite {
                id: "5b17e5ef".to_string(),
                fps: None
            }
        );
        assert_eq!(round(head), head);

        // set: round trips right after the `I` line
        for f in [0u8, 1, 12, crate::sprite::SPRITE_MAX_FPS] {
            let mut wire = String::from(head);
            wire.push_str("A ");
            push_u32(&mut wire, f as u32);
            wire.push('\n');
            let s = parse(&wire).expect(&wire);
            assert_eq!(
                s.layers[0].body,
                LayerBody::Sprite {
                    id: "5b17e5ef".to_string(),
                    fps: Some(f)
                }
            );
            assert_eq!(round(&wire), wire);
        }

        // an `A` with no `I` is still a legal override
        let idless = "S 0000000a n\nL sprite 0 0 0 0 normal 100 none fill 1\nA 3\n";
        assert_eq!(round(idless), idless);

        // out of range, non-numeric, and on a layer it cannot apply to
        assert_eq!(
            parse(&alloc::format!("{head}A 31\n")).unwrap_err(),
            "scene: line 4: sprite fps must be 0..30 \"31\""
        );
        assert!(parse(&alloc::format!("{head}A -1\n")).is_err());
        assert_eq!(
            parse(&alloc::format!("{head}A x\n")).unwrap_err(),
            "scene: line 4: sprite fps is not an integer \"x\""
        );
        assert_eq!(
            parse("S 0000000a n\nL pat 0 0 0 0 normal 100 none fill 1\nA 4\n").unwrap_err(),
            "scene: line 3: A on a non-sprite layer"
        );
    }

    /// The JSON member is present only when the override is set.
    #[test]
    fn json_carries_the_sprite_fps_override_only_when_set() {
        let head = "S 0000000a n\nL sprite 0 0 0 0 normal 100 none fill 1\nI 5b17e5ef\n";
        let json = |wire: &str| {
            let mut out = String::new();
            push_json(&parse(wire).expect(wire), &mut out);
            out
        };
        assert!(json(head).ends_with("\"sprite\":{\"id\":\"5b17e5ef\"}}]}"));
        assert!(json(&alloc::format!("{head}A 0\n"))
            .ends_with("\"sprite\":{\"id\":\"5b17e5ef\",\"fps\":0}}]}"));
        assert!(json(&alloc::format!("{head}A 30\n"))
            .ends_with("\"sprite\":{\"id\":\"5b17e5ef\",\"fps\":30}}]}"));
    }

    #[test]
    fn unknown_lines_and_blank_lines_are_ignored() {
        let wire = concat!(
            "S 0000000a n\n",
            "Z whatever a newer console sends\n",
            "\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "Q 1 2 3\n",
            "K 112233\n",
        );
        assert_eq!(
            round(wire),
            "S 0000000a n\nL color 0 0 0 0 normal 100 none fill 1\nK 112233\n"
        );
    }

    #[test]
    fn a_binding_line_for_the_wrong_layer_type_is_ignored() {
        let wire = "S 0000000a n\nL color 0 0 0 0 normal 100 none fill 1\nT lit HI\nK 010203\n";
        let s = parse(wire).unwrap();
        assert_eq!(s.layers[0].body, LayerBody::Color([1, 2, 3]));
    }

    #[test]
    fn dash_means_assign_an_id() {
        let s = parse("S - Fresh\nL color 0 0 0 0 normal 100 none fill 1\n").unwrap();
        assert_eq!(s.id, "");
        assert_eq!(s.name, "Fresh");
        let mut out = String::new();
        serialize(&s, &mut out);
        assert!(out.starts_with("S - Fresh\n"), "{out}");
    }

    #[test]
    fn ids_are_eight_lowercase_hex() {
        assert!(valid_id("0123abcd"));
        assert!(!valid_id("0123ABCD"));
        assert!(!valid_id("0123abc"));
        assert!(!valid_id("0123abcde"));
        assert!(!valid_id("0123abcg"));
        assert_eq!(
            parse("S 0123ABCD x\n").unwrap_err(),
            "scene: line 1: bad scene id \"0123ABCD\""
        );
        assert_eq!(
            parse("S 0000000a n\nL pat 0 0 0 0 normal 100 none fill 1\nI zz\n").unwrap_err(),
            "scene: line 3: bad pattern id \"zz\""
        );
    }

    #[test]
    fn names_are_length_capped() {
        let long: String = core::iter::repeat('x').take(65).collect();
        let mut wire = "S 0000000a ".to_string();
        wire.push_str(&long);
        assert_eq!(
            parse(&wire).unwrap_err(),
            "scene: line 1: scene name is over 64 bytes"
        );
        let ok: String = core::iter::repeat('x').take(64).collect();
        let mut wire = "S 0000000a ".to_string();
        wire.push_str(&ok);
        assert_eq!(parse(&wire).unwrap().name.len(), 64);

        let long: String = core::iter::repeat('y').take(33).collect();
        let mut wire = "S 0000000a n\nL color 0 0 0 0 normal 100 none fill 1\nN ".to_string();
        wire.push_str(&long);
        assert_eq!(
            parse(&wire).unwrap_err(),
            "scene: line 3: layer name is over 32 bytes"
        );
    }

    /// Every rejection names the line it happened on — that string is what
    /// the API hands the console, so it is part of the contract.
    #[test]
    fn errors_name_their_line() {
        let head = "S 0000000a n\nL pat 0 0 0 0 normal 100 none fill 1\n";
        let cases: &[(&str, &str)] = &[
            ("L nope 0 0 0 0 normal 100 none fill 1", "unknown layer type \"nope\""),
            ("L pat 0 0 0 0 foo 100 none fill 1", "unknown blend \"foo\""),
            ("L pat 0 0 0 0 normal 100 foo fill 1", "unknown key \"foo\""),
            ("L pat 0 0 0 0 normal 100 none foo 1", "unknown fit \"foo\""),
            ("L pat 0 0 0 0 normal 101 none fill 1", "opacity must be 0..100 \"101\""),
            ("L pat 0 0 0 0 normal 100 none fill 16", "flags must be 0..15 \"16\""),
            ("L pat 0 0 0 0 normal x none fill 1", "opacity is not an integer \"x\""),
            ("L pat 0 0 0 0 normal 100 none fill", "L needs 10 fields, missing flags"),
            ("P sideways", "unknown projection \"sideways\""),
            ("R 50 0:000000", "a ramp needs at least two stops"),
            ("R 50 255:ffffff 0:000000", "ramp stops must ascend by position"),
            ("R 50 0:00000 255:ffffff", "a ramp stop colour is rrggbb \"0:00000\""),
            ("R 101 0:000000 255:ffffff", "ramp amount must be 0..100 \"101\""),
            ("T nope", "unknown text source \"nope\""),
            ("T clock HH:mm", "unknown clock format \"HH:mm\""),
            ("T slot 8", "a text slot is 0..7 \"8\""),
            ("F comic ffffff l none 0", "unknown font \"comic\""),
            ("F regular fff l none 0", "text colour is rrggbb \"fff\""),
            ("F regular ffffff m none 0", "unknown align \"m\""),
            ("F regular ffffff l sideways 0", "unknown scroll \"sideways\""),
            ("C speed nope", "control values are raw 16.16 integers \"nope\""),
            ("K nope", "wash colour is rrggbb \"nope\""),
            ("M 0 1 2 3 wobble", "bad M field \"wobble\""),
            ("M 0 1 2 3", "M needs <ms> <x> <y> <opacity> <ease> [<frame>]"),
            ("M -1 1 2 3 linear", "bad M field \"-1\""),
            ("M 3600001 1 2 3 linear", "bad M field \"3600001\""),
            ("M = 1 2 3 linear", "bad M field \"=\""),
            ("M 0 40000 2 3 linear", "bad M field \"40000\""),
            ("M 0 1 * 3 linear", "bad M field \"*\""),
            ("M 0 1 2 101 linear", "bad M field \"101\""),
            ("M 0 1 2 3 linear 256", "bad M field \"256\""),
            ("M 0 1 2 3 linear =", "bad M field \"=\""),
        ];
        for (line, want) in cases {
            let mut wire = head.to_string();
            wire.push_str(line);
            wire.push('\n');
            let got = parse(&wire).unwrap_err();
            let mut expect = "scene: line 3: ".to_string();
            expect.push_str(want);
            assert_eq!(got, expect, "for {line}");
        }
    }

    #[test]
    fn keyframes_round_trip_on_every_layer_kind() {
        let wire = concat!(
            "S 0000000a keyed\n",
            "L pat 0 0 16 16 normal 100 none fill 1\n",
            "I 0123abcd\n",
            "M 0 0 0 100 linear\n",
            "M 500 = -4 = step\n",
            "M 1000 8 = 0 in\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
            "T lit HI\n",
            "M 0 = = = out\n",
            "M 1 = = = inout\n",
            "L sprite 4 40 22 22 normal 100 none contain 1\n",
            "I 0123abce\n",
            "A 6\n",
            "M 0 4 40 0 sine 2\n",
            "M 3000 4 20 100 back\n",
            "M 3000 4 20 100 bounce 0\n",
            "M 20000 4 40 0 elastic 255\n",
            "L color 0 0 0 0 add 50 none fill 1\n",
            "K ff8800\n",
            "M 3600000 -32768 32767 0 linear\n",
        );
        assert_eq!(round(wire), wire);
        let s = parse(wire).unwrap();
        assert_eq!(parse(&round(wire)).unwrap(), s);
        assert_eq!(s.layers[0].keys.len(), 3);
        assert_eq!(
            s.layers[0].keys[1],
            Keyframe {
                ms: 500,
                x: None,
                y: Some(-4),
                opacity: None,
                ease: Ease::Step,
                frame: None,
            }
        );
        assert_eq!(s.layers[2].keys[0].frame, Some(2));
        // every ease spelling parses back to itself
        for e in Ease::ALL {
            let mut w = String::from("S 0000000a e\nL color 0 0 0 0 normal 100 none fill 1\nM 0 = = = ");
            w.push_str(e.as_str());
            assert_eq!(parse(&w).unwrap().layers[0].keys[0].ease, e);
        }
    }

    #[test]
    fn keyframe_lines_canonicalise_and_tolerate_a_newer_console() {
        // aligned columns (a library source's habit) and a token past
        // `frame` from some later grammar both parse; serialize is canonical
        let wire = "S 0000000a k\nL color 0 0 0 0 normal 100 none fill 1\nM  0    4 40  0   linear\nM 9 1 2 3 out 4 later\n";
        assert_eq!(
            round(wire),
            "S 0000000a k\nL color 0 0 0 0 normal 100 none fill 1\nM 0 4 40 0 linear\nM 9 1 2 3 out 4\n"
        );
    }

    #[test]
    fn a_keyframe_track_is_capped_and_ordered() {
        let head = "S 0000000a k\nL color 0 0 0 0 normal 100 none fill 1\n";
        let mut wire = String::from(head);
        for i in 0..MAX_KEYS {
            wire.push_str("M ");
            push_u32(&mut wire, i as u32 * 10);
            wire.push_str(" 0 0 0 linear\n");
        }
        assert_eq!(parse(&wire).unwrap().layers[0].keys.len(), MAX_KEYS);
        wire.push_str("M 999 0 0 0 linear\n");
        assert_eq!(parse(&wire).unwrap_err(), "scene: line 19: too many keyframes (max 16)");
        let mut back = String::from(head);
        back.push_str("M 100 0 0 0 linear\nM 100 1 1 1 step\nM 50 0 0 0 linear\n");
        assert_eq!(
            parse(&back).unwrap_err(),
            "scene: line 5: keyframe times must not go backwards"
        );
        // an M before any L binds to nothing and is ignored, like any
        // binding line there
        assert!(parse("S 0000000a k\nM 0 0 0 0 linear\n").unwrap().layers.is_empty());
    }

    #[test]
    fn json_carries_keys_only_when_the_layer_has_a_track() {
        let s = parse(concat!(
            "S 0000000a k\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "L sprite 1 2 0 0 normal 100 none fill 1\n",
            "M 0 = 5 0 linear 3\n",
            "M 250 7 = = elastic\n",
        ))
        .unwrap();
        let mut out = String::new();
        push_json(&s, &mut out);
        assert_eq!(out.matches("\"keys\"").count(), 1);
        assert!(out.contains(concat!(
            "\"rot180\":false,\"keys\":[{\"ms\":0,\"y\":5,\"opacity\":0,\"ease\":\"linear\",\"frame\":3},",
            "{\"ms\":250,\"x\":7,\"ease\":\"elastic\"}],\"sprite\":{"
        )));
    }

    #[test]
    fn x_clip_round_trips_and_ignores_words_it_does_not_know() {
        let wire = concat!(
            "S 0000000a clipped\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
            "T lit HI\n",
            "L pat 0 0 0 0 normal 100 none fill 1\n",
            "I 0123abcd\n",
            "X clip\n",
            "M 0 = = = linear\n",
        );
        assert_eq!(round(wire), wire);
        let s = parse(wire).unwrap();
        assert!(!s.layers[1].clip && s.layers[2].clip);
        // a newer console's extra words: `clip` is still read, the rest
        // dropped; an X line with no word this host knows changes nothing
        let s = parse("S 0000000a c\nL color 0 0 0 0 normal 100 none fill 1\nX feather clip\nX glow\n").unwrap();
        assert!(s.layers[0].clip);
        let s = parse("S 0000000a c\nL color 0 0 0 0 normal 100 none fill 1\nX glow\n").unwrap();
        assert!(!s.layers[0].clip);
        let mut out = String::new();
        push_json(&parse(wire).unwrap(), &mut out);
        assert_eq!(out.matches("\"clip\":true").count(), 1);
        assert!(out.contains("\"rot180\":false,\"clip\":true,\"keys\":["));
    }

    #[test]
    fn the_design_size_round_trips_and_sits_after_s() {
        let wire = concat!(
            "S 0000000a fire clock\n",
            "U 32 32\n",
            "L text 0 16 0 0 normal 100 none fill 1\n",
            "T clock HH:MM\n",
            "L sprite 0 0 0 0 normal 100 none tile 1\n",
            "I 0123abcd\n",
            "X abs\n",
            "L pat 0 0 0 0 lighten 100 none fill 1\n",
            "X clip abs\n",
        );
        assert_eq!(round(wire), wire);
        let s = parse(wire).unwrap();
        assert_eq!(s.space, Some((32, 32)));
        assert_eq!(
            s.layers.iter().map(|l| (l.clip, l.abs)).collect::<Vec<_>>(),
            [(false, false), (false, true), (true, true)]
        );
        // canonical X: one line, `clip` before `abs`, unknown words dropped
        let s = parse("S 0000000a c\nL color 0 0 0 0 normal 100 none fill 1\nX glow abs\nX clip\n")
            .unwrap();
        assert!(s.layers[0].clip && s.layers[0].abs);
        let mut out = String::new();
        serialize(&s, &mut out);
        assert!(out.ends_with("L color 0 0 0 0 normal 100 none fill 1\nX clip abs\n"));
        // the limits, and a bare scene with only S and U
        assert_eq!(parse("S - a\nU 1 4096\n").unwrap().space, Some((1, 4096)));
        assert_eq!(parse("S - a\n").unwrap().space, None);
    }

    #[test]
    fn a_bad_u_line_names_itself() {
        let bad = |u: &str| {
            parse(&alloc::format!(
                "S - a\n{u}\nL color 0 0 0 0 normal 100 none fill 1\n"
            ))
            .unwrap_err()
        };
        assert_eq!(bad("U 0 32"), "scene: line 2: bad U field \"0\"");
        assert_eq!(bad("U 32 4097"), "scene: line 2: bad U field \"4097\"");
        assert_eq!(bad("U 32"), "scene: line 2: bad U field \"\"");
        assert_eq!(bad("U 32 x"), "scene: line 2: bad U field \"x\"");
        assert_eq!(bad("U -1 32"), "scene: line 2: bad U field \"-1\"");
        assert_eq!(
            parse("S - a\nL color 0 0 0 0 normal 100 none fill 1\nU 32 32\n").unwrap_err(),
            "scene: line 3: a U line after the first L line"
        );
        assert_eq!(
            parse("U 32 32\nS - a\n").unwrap_err(),
            "scene: line 1: a binding line before the S line"
        );
        // trailing words are a newer console's, like every line's
        assert_eq!(parse("S - a\nU 32 16 rel\n").unwrap().space, Some((32, 16)));
    }

    #[test]
    fn json_carries_space_and_abs_only_when_set() {
        let mut out = String::new();
        push_json(
            &parse("S 0000000a n\nU 32 16\nL color 0 0 0 0 normal 100 none fill 1\nX clip abs\n")
                .unwrap(),
            &mut out,
        );
        assert!(
            out.starts_with("{\"id\":\"0000000a\",\"name\":\"n\",\"space\":[32,16],\"layers\":["),
            "{out}"
        );
        assert!(
            out.contains("\"rot180\":false,\"clip\":true,\"abs\":true,\"color\""),
            "{out}"
        );
        let mut out = String::new();
        push_json(
            &parse("S 0000000a n\nL color 0 0 0 0 normal 100 none fill 1\n").unwrap(),
            &mut out,
        );
        assert!(!out.contains("space") && !out.contains("abs"), "{out}");
    }

    #[test]
    fn a_block_needs_its_s_line() {
        assert_eq!(
            parse("L color 0 0 0 0 normal 100 none fill 1\n").unwrap_err(),
            "scene: line 1: an L line before the S line"
        );
        assert_eq!(parse("").unwrap_err(), "scene: line 1: expected an S line");
        assert_eq!(
            parse("S 0000000a a\nS 0000000b b\n").unwrap_err(),
            "scene: line 2: a second S line in one scene block"
        );
    }

    #[test]
    fn parse_all_splits_on_s_lines_and_counts_blob_lines() {
        let blob = concat!(
            "S 0000000a first\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "S 0000000b second\n",
            "L pat 0 0 0 0 normal 100 none fill 1\n",
            "I 0123abcd\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
        );
        let all = parse_all(blob).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "first");
        assert_eq!(all[1].layers.len(), 2);
        assert_eq!(pattern_layers(&all[0]), 0);
        assert_eq!(pattern_layers(&all[1]), 1);
        // a whole blob re-serializes to itself
        let mut out = String::new();
        for s in &all {
            serialize(s, &mut out);
        }
        assert_eq!(out, blob);
        // error line numbers are blob-global, not block-local
        let bad = "S 0000000a a\nL color 0 0 0 0 normal 100 none fill 1\nS 0000000b b\nL pat 0 0 0 0 zzz 100 none fill 1\n";
        assert_eq!(
            parse_all(bad).unwrap_err(),
            "scene: line 4: unknown blend \"zzz\""
        );
        assert!(parse_all("").unwrap().is_empty());
    }

    #[test]
    fn json_carries_the_api_shape() {
        let wire = concat!(
            "S 0000000a My \"scene\"\n",
            "L pat 1 2 16 8 multiply 60 luma tile 7\n",
            "I 0123abcd\n",
            "C speed 32768\n",
            "P xy\n",
            "R 80 0:000000 255:ffffff\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
            "T slot 3\n",
            "L sprite 0 0 0 0 normal 100 black fill 1\n",
            "I 0123abce\n",
            "L color 0 0 0 0 add 50 none fill 1\n",
            "K ff8800\n",
        );
        let s = parse(wire).unwrap();
        let mut out = String::new();
        push_json(&s, &mut out);
        assert_eq!(
            out,
            concat!(
                "{\"id\":\"0000000a\",\"name\":\"My \\\"scene\\\"\",\"layers\":[",
                "{\"type\":\"pat\",\"name\":\"\",\"x\":1,\"y\":2,\"w\":16,\"h\":8,\"blend\":\"multiply\",",
                "\"opacity\":60,\"key\":\"luma\",\"fit\":\"tile\",\"visible\":true,\"flipx\":true,\"flipy\":true,",
                "\"rot180\":false,\"pat\":{\"id\":\"0123abcd\",\"controls\":{\"speed\":[0.5]},\"proj\":\"xy\",",
                "\"ramp\":{\"pct\":80,\"stops\":[[0,\"000000\"],[255,\"ffffff\"]]}}},",
                "{\"type\":\"text\",\"name\":\"Text\",\"x\":0,\"y\":0,\"w\":0,\"h\":0,\"blend\":\"normal\",",
                "\"opacity\":100,\"key\":\"none\",\"fit\":\"fill\",\"visible\":true,\"flipx\":false,",
                "\"flipy\":false,\"rot180\":false,\"text\":{\"source\":\"slot\",\"slot\":3,\"font\":\"regular\",",
                "\"color\":\"ffffff\",\"align\":\"l\",\"scroll\":\"none\",\"speed\":0}},",
                "{\"type\":\"sprite\",\"name\":\"Sprite\",\"x\":0,\"y\":0,\"w\":0,\"h\":0,\"blend\":\"normal\",",
                "\"opacity\":100,\"key\":\"black\",\"fit\":\"fill\",\"visible\":true,\"flipx\":false,",
                "\"flipy\":false,\"rot180\":false,\"sprite\":{\"id\":\"0123abce\"}},",
                "{\"type\":\"color\",\"name\":\"Color\",\"x\":0,\"y\":0,\"w\":0,\"h\":0,\"blend\":\"add\",",
                "\"opacity\":50,\"key\":\"none\",\"fit\":\"fill\",\"visible\":true,\"flipx\":false,",
                "\"flipy\":false,\"rot180\":false,\"color\":\"ff8800\"}]}"
            )
        );
    }

    // ---- the fallible response reservation (Gitea #728) ----

    /// Wire bodies that between them exercise every layer kind, every
    /// optional block and every widest-case value `push_json` can meet.
    fn bound_fixtures() -> alloc::vec::Vec<String> {
        alloc::vec![
            // the smallest legal record
            String::from("S 0000000a\n"),
            // names at their ceilings, in bytes that ESCAPE
            String::from(concat!(
                "S 0000000a \"\\\t0123456789012345678901234567890123456789012345\n",
                "L color 0 0 0 0 normal 100 none fill 1\n",
                "N \"\\\t0123456789012345678901234\n",
                "K ff8800\n",
            )),
            // every layer kind at once, with the widest enum spellings
            String::from(concat!(
                "S 0000000a every kind\n",
                "L pat -32768 -32768 65535 65535 multiply 100 black contain 15\n",
                "N Base\n",
                "I 0123abcd\n",
                "C sliderSpeedOfTheThing 1 -1 65536 2147483647 -2147483648\n",
                "C hue -1\n",
                "P xy\n",
                "R 100 0:000000 128:ff0000 255:ffffff\n",
                "L text 0 0 0 0 lighten 100 luma tile 15\n",
                "N Ticker\n",
                "T clock YYYY-MM-DD\n",
                "F regular ffffff c bounce 65535\n",
                "L text 0 0 0 0 normal 100 none fill 1\n",
                "T lit HELLO \"WORLD\" \u{1f680}\n",
                "L sprite 0 0 0 0 add 50 black fill 1\n",
                "I 0123abce\n",
                "A 30\n",
                "L color 0 0 0 0 mask 0 luma tile 0\n",
                "K ff8800\n",
            )),
            // a keyframe track at its cap, every field at its widest, on a
            // clip + abs layer in a scene with the widest design size
            {
                let mut b = String::from("S 0000000a keyed\nU 4096 4096\nL color 0 0 0 0 normal 100 none fill 1\nX clip abs\n");
                for _ in 0..MAX_KEYS {
                    b.push_str("M 3600000 -32768 -32768 100 elastic 255\n");
                }
                b
            },
            // a ramp at the stop cap, which is the widest optional block
            {
                let mut b = String::from("S 0000000a ramped\nL pat 0 0 0 0 normal 100 none fill 1\nI 0123abcd\nR 100");
                for i in 0..MAX_RAMP_STOPS {
                    b.push(' ');
                    push_u32(&mut b, (i * 255 / (MAX_RAMP_STOPS - 1)) as u32);
                    b.push_str(":ff8800");
                }
                b.push('\n');
                b
            },
        ]
    }

    /// [`json_bound`] must never be smaller than what [`push_json`] writes.
    /// If this fails, `GET /api/scenes` is one `String` doubling away from
    /// an allocator panic on a device again — raise the constants at the top
    /// of the JSON section, do NOT relax the test.
    #[test]
    fn push_json_never_outgrows_its_bound() {
        for wire in bound_fixtures() {
            let s = parse(&wire).expect(&wire);
            let mut out = String::new();
            push_json(&s, &mut out);
            let bound = json_bound(&s);
            assert!(
                out.len() <= bound,
                "{} B written, bound {}: {wire}",
                out.len(),
                bound
            );
            // and it must stay a USEFUL bound: a reservation far larger than
            // the body is a refusal the device did not have to make
            assert!(
                bound <= out.len() * 2 + 256,
                "bound {} is loose for a {} B body: {wire}",
                bound,
                out.len()
            );
        }
    }

    /// The property the bound exists for: a body reserved from it is ONE
    /// allocation, start to finish.
    #[test]
    fn a_bound_reservation_never_reallocates() {
        for wire in bound_fixtures() {
            let s = parse(&wire).expect(&wire);
            let mut out = crate::jsonview::try_body(json_bound(&s)).expect("host heap");
            let cap = out.capacity();
            push_json(&s, &mut out);
            assert_eq!(out.capacity(), cap, "push_json grew its reservation: {wire}");
            // and the bytes are the same ones an unreserved build produces
            let mut plain = String::new();
            push_json(&s, &mut plain);
            assert_eq!(out, plain);
        }
    }

    #[test]
    fn flags_decode_to_the_four_booleans() {
        let s = parse("S 0000000a n\nL color 0 0 0 0 normal 100 none fill 14\n").unwrap();
        let st = s.layers[0].style;
        assert!(!st.visible && st.flipx && st.flipy && st.rot180);
        assert_eq!(st.flags(), 14);
    }
}
