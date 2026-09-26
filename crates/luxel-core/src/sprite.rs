//! The sprite record — `LXSP` v1 (Gitea #740).
//!
//! A sprite is a first-class store record: a palette-indexed pixel image
//! with frames, and NOT a pattern. This module is the one reader and the
//! one validator of that record; ONE byte layout is the flash bytes, the
//! wire bytes (`GET`/`POST /api/sprites…` bodies), the playground's stored
//! bytes and the wasm compositor's input. The firmware's compositor reads
//! texels straight out of the mapped record, so a sprite layer holds no
//! engine and costs no RAM (docs/spec/scenes.md §4).
//!
//! Layout, little-endian, byte-addressable:
//!
//! ```text
//! off  size        field
//! 0    4           magic  "LXSP"
//! 4    1           version = 1
//! 5    1           w        1..=64
//! 6    1           h        1..=64
//! 7    1           frames   1..=255
//! 8    1           fps      0..=30   (0 = static)
//! 9    1           colors   0..=255  (palette entries)
//! 10   1           name_len 1..=64   (UTF-8 bytes)
//! 11   1           flags    0        (reserved; readers ignore, writers write 0)
//! 12   name_len    name
//! +    3*colors    palette  [r,g,b] × colors, RGB888
//! +    w*h*frames  index    one byte per texel, frame-major then row-major;
//!                           0 = TRANSPARENT, k = palette[k-1]
//! ```
//!
//! Transparency is index 0, not black: an opaque black texel is a palette
//! colour like any other. (The sprite-tagged PATTERN format this replaced
//! keyed on `v == 0`; the web console converts those records once — #740.)

/// `"LXSP"`.
pub const SPRITE_MAGIC: [u8; 4] = *b"LXSP";
pub const SPRITE_VERSION: u8 = 1;
/// Fixed header bytes before the name.
pub const SPRITE_HDR: usize = 12;
/// Largest edge, in texels.
pub const SPRITE_MAX_EDGE: usize = 64;
pub const SPRITE_MAX_FPS: u8 = 30;
pub const SPRITE_MAX_FRAMES: usize = 255;
/// Palette entries; the index byte's 255 non-zero values.
pub const SPRITE_MAX_COLORS: usize = 255;
/// Longest name, in bytes — the store's `MAX_NAME`.
pub const SPRITE_MAX_NAME: usize = 64;
/// Largest record the store and the routes accept. The device's HTTP
/// request buffer is 16 KiB, so a bigger record could never arrive; a
/// 64×64 sprite fits 3 frames, 32×32 fits 15, 16×16 fits 63.
pub const SPRITE_MAX_BYTES: usize = 16 * 1024;

const O_VER: usize = 4;
const O_W: usize = 5;
const O_H: usize = 6;
const O_FRAMES: usize = 7;
const O_FPS: usize = 8;
const O_COLORS: usize = 9;
const O_NAME_LEN: usize = 10;

/// Exact byte length a record of these dimensions has.
pub const fn record_len(name_len: usize, colors: usize, w: usize, h: usize, frames: usize) -> usize {
    SPRITE_HDR + name_len + 3 * colors + w * h * frames
}

/// A validated view over a sprite record's bytes — everything
/// `compose::blit_sprite` needs, borrowed in place (on the device: the
/// memory-mapped store).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteView<'a> {
    pub w: u8,
    pub h: u8,
    pub frames: u8,
    pub fps: u8,
    pub name: &'a str,
    palette: &'a [u8],
    index: &'a [u8],
}

impl<'a> SpriteView<'a> {
    /// Parse and validate a whole record. `None` for anything malformed —
    /// [`check`] says what.
    pub fn parse(bytes: &'a [u8]) -> Option<SpriteView<'a>> {
        check(bytes).ok()?;
        let name_len = bytes[O_NAME_LEN] as usize;
        let colors = bytes[O_COLORS] as usize;
        let name = core::str::from_utf8(&bytes[SPRITE_HDR..SPRITE_HDR + name_len]).ok()?;
        let pal_at = SPRITE_HDR + name_len;
        let idx_at = pal_at + 3 * colors;
        Some(SpriteView {
            w: bytes[O_W],
            h: bytes[O_H],
            frames: bytes[O_FRAMES],
            fps: bytes[O_FPS],
            name,
            palette: &bytes[pal_at..idx_at],
            index: &bytes[idx_at..],
        })
    }

    /// Palette entries.
    #[inline]
    pub fn colors(&self) -> usize {
        self.palette.len() / 3
    }

    /// Texels per frame.
    #[inline]
    pub fn texels(&self) -> usize {
        self.w as usize * self.h as usize
    }

    /// Texels over every frame — the index's length.
    #[inline]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Palette entry `k` (0-based), RGB888.
    #[inline]
    pub fn color(&self, k: usize) -> Option<[u8; 3]> {
        let at = k.checked_mul(3)?;
        match self.palette.get(at..at + 3) {
            Some(&[r, g, b]) => Some([r, g, b]),
            _ => None,
        }
    }

    /// RGB of texel `i` (frame-major, row-major), `None` when transparent
    /// or out of range.
    #[inline]
    pub fn texel(&self, i: usize) -> Option<[u8; 3]> {
        let k = *self.index.get(i)? as usize;
        if k == 0 {
            return None;
        }
        self.color(k - 1)
    }

    /// The raw palette index of texel `i` (0 = transparent).
    #[inline]
    pub fn index_at(&self, i: usize) -> u8 {
        self.index.get(i).copied().unwrap_or(0)
    }

    /// The frame shown `elapsed_ms` into playback: `(elapsed·fps/1000) mod
    /// frames`, and always 0 for a static (`fps == 0`) or single-frame
    /// sprite. The clock rule the device and the playground share.
    pub fn frame_at(&self, elapsed_ms: u32) -> u8 {
        if self.frames <= 1 || self.fps == 0 {
            return 0;
        }
        ((elapsed_ms as u64 * self.fps as u64 / 1000) % self.frames as u64) as u8
    }
}

/// Why a record is bad, as the user-facing reason the routes answer with —
/// every message starts `sprite: `. [`SpriteView::parse`] is the fast
/// yes/no; this is the diagnosis.
pub fn check(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.len() < SPRITE_HDR + 1 {
        return Err("sprite: record is too short");
    }
    if bytes[..4] != SPRITE_MAGIC {
        return Err("sprite: bad magic");
    }
    if bytes[O_VER] != SPRITE_VERSION {
        return Err("sprite: unknown record version");
    }
    if bytes.len() > SPRITE_MAX_BYTES {
        return Err("sprite: over the 16 KiB cap");
    }
    let (w, h, frames) = (bytes[O_W] as usize, bytes[O_H] as usize, bytes[O_FRAMES] as usize);
    if w == 0 || h == 0 || w > SPRITE_MAX_EDGE || h > SPRITE_MAX_EDGE {
        return Err("sprite: 1..64 texels on each edge");
    }
    if frames == 0 {
        return Err("sprite: at least one frame");
    }
    if bytes[O_FPS] > SPRITE_MAX_FPS {
        return Err("sprite: fps 0..30");
    }
    let colors = bytes[O_COLORS] as usize;
    let name_len = bytes[O_NAME_LEN] as usize;
    if name_len == 0 || name_len > SPRITE_MAX_NAME {
        return Err("sprite: name must be 1..=64 bytes");
    }
    if bytes.len() != record_len(name_len, colors, w, h, frames) {
        return Err("sprite: length does not match its header");
    }
    if core::str::from_utf8(&bytes[SPRITE_HDR..SPRITE_HDR + name_len]).is_err() {
        return Err("sprite: name is not utf-8");
    }
    let idx_at = SPRITE_HDR + name_len + 3 * colors;
    if bytes[idx_at..].iter().any(|&k| k as usize > colors) {
        return Err("sprite: index out of palette");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Assemble a record from parts — what the web codec's `encodeSprite`
    /// produces, written here by hand so the two cannot share a bug.
    pub fn build(name: &str, w: u8, h: u8, frames: u8, fps: u8, palette: &[[u8; 3]], index: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&SPRITE_MAGIC);
        v.push(SPRITE_VERSION);
        v.extend_from_slice(&[w, h, frames, fps, palette.len() as u8, name.len() as u8, 0]);
        v.extend_from_slice(name.as_bytes());
        for c in palette {
            v.extend_from_slice(c);
        }
        v.extend_from_slice(index);
        v
    }

    fn heart() -> Vec<u8> {
        // 3×2, two frames, two colours; frame 1 is frame 0 with the corner off
        build(
            "Heart",
            3,
            2,
            2,
            10,
            &[[255, 0, 0], [0, 0, 0]],
            &[1, 0, 1, 2, 1, 2, 1, 0, 1, 2, 1, 0],
        )
    }

    #[test]
    fn a_record_parses_and_reads_back_texel_for_texel() {
        let rec = heart();
        assert_eq!(rec.len(), record_len(5, 2, 3, 2, 2));
        let sp = SpriteView::parse(&rec).expect("parse");
        assert_eq!((sp.w, sp.h, sp.frames, sp.fps), (3, 2, 2, 10));
        assert_eq!(sp.name, "Heart");
        assert_eq!(sp.colors(), 2);
        assert_eq!(sp.texels(), 6);
        assert_eq!(sp.len(), 12);
        assert_eq!(sp.texel(0), Some([255, 0, 0]));
        assert_eq!(sp.texel(1), None, "index 0 is transparent");
        // an opaque BLACK texel is a colour, not the key
        assert_eq!(sp.texel(3), Some([0, 0, 0]));
        assert_eq!(sp.texel(11), None);
        assert_eq!(sp.texel(12), None, "past the end");
        assert_eq!(sp.index_at(3), 2);
        assert_eq!(sp.color(5), None);
    }

    #[test]
    fn the_frame_clock_follows_fps_and_wraps() {
        let rec = heart();
        let sp = SpriteView::parse(&rec).unwrap();
        // 10 fps, 2 frames: 0..100 ms → 0, 100..200 → 1, 200.. → 0
        assert_eq!(sp.frame_at(0), 0);
        assert_eq!(sp.frame_at(99), 0);
        assert_eq!(sp.frame_at(100), 1);
        assert_eq!(sp.frame_at(250), 0);
        // static and single-frame sprites never advance
        let still = build("s", 1, 1, 2, 0, &[[1, 2, 3]], &[1, 1]);
        assert_eq!(SpriteView::parse(&still).unwrap().frame_at(5000), 0);
        let one = build("s", 1, 1, 1, 30, &[[1, 2, 3]], &[1]);
        assert_eq!(SpriteView::parse(&one).unwrap().frame_at(5000), 0);
        // no overflow at large elapsed values
        assert!(sp.frame_at(u32::MAX) < 2);
    }

    #[test]
    fn every_malformation_is_named() {
        let good = heart();
        assert_eq!(check(&good), Ok(()));

        let mut b = good.clone();
        b[0] = b'X';
        assert_eq!(check(&b), Err("sprite: bad magic"));

        let mut b = good.clone();
        b[O_VER] = 2;
        assert_eq!(check(&b), Err("sprite: unknown record version"));

        assert_eq!(check(&good[..8]), Err("sprite: record is too short"));

        for (at, v) in [(O_W, 0u8), (O_W, 65), (O_H, 0), (O_H, 65)] {
            let mut b = good.clone();
            b[at] = v;
            assert_eq!(check(&b), Err("sprite: 1..64 texels on each edge"), "{at}={v}");
        }

        let mut b = good.clone();
        b[O_FRAMES] = 0;
        assert_eq!(check(&b), Err("sprite: at least one frame"));

        let mut b = good.clone();
        b[O_FPS] = 31;
        assert_eq!(check(&b), Err("sprite: fps 0..30"));

        let mut b = good.clone();
        b[O_NAME_LEN] = 0;
        assert_eq!(check(&b), Err("sprite: name must be 1..=64 bytes"));

        // one byte short / long
        let mut b = good.clone();
        b.pop();
        assert_eq!(check(&b), Err("sprite: length does not match its header"));
        let mut b = good.clone();
        b.push(0);
        assert_eq!(check(&b), Err("sprite: length does not match its header"));

        let mut b = good.clone();
        let last = b.len() - 1;
        b[last] = 3; // two colours → 3 is out of the palette
        assert_eq!(check(&b), Err("sprite: index out of palette"));
        assert!(SpriteView::parse(&b).is_none());

        let mut b = good.clone();
        b[SPRITE_HDR] = 0xff; // the name's first byte
        assert_eq!(check(&b), Err("sprite: name is not utf-8"));

        // the cap: a 64×64×4 record is 16 KiB + header
        let big = build("big", 64, 64, 4, 0, &[], &alloc::vec![0u8; 64 * 64 * 4]);
        assert!(big.len() > SPRITE_MAX_BYTES);
        assert_eq!(check(&big), Err("sprite: over the 16 KiB cap"));
        // …and 3 frames fit
        let ok = build("big", 64, 64, 3, 0, &[], &alloc::vec![0u8; 64 * 64 * 3]);
        assert!(ok.len() <= SPRITE_MAX_BYTES);
        assert_eq!(check(&ok), Ok(()));
    }

    #[test]
    fn a_palette_free_record_is_all_transparent() {
        let rec = build("blank", 2, 2, 1, 0, &[], &[0, 0, 0, 0]);
        let sp = SpriteView::parse(&rec).unwrap();
        assert_eq!(sp.colors(), 0);
        assert!((0..4).all(|i| sp.texel(i).is_none()));
    }
}
