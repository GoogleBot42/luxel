//! Projection — how a pattern made for one dimensionality is shown on a
//! Layout of another (Gitea #473, #538, design proposal §5.4d).
//!
//! A Layout has `dims` (1 = strip, 2 = matrix/plane, 3 = lattice/cloud) and a
//! pattern has its own preferred dims ([`crate::engine::Engine::preferred_dims`]).
//! When they differ the engine must decide what coordinates — and how many
//! render calls — the pattern gets. That decision is the *projection*, and it
//! keys off **dims alone**: an axis is a coordinate substitution and does not
//! care whether the pixels sit on a regular grid. "Custom map" is a
//! coordinate SOURCE, not a dimensionality.
//!
//! **A Layout only ever shows a pattern of its own dimensionality or lower**
//! (#538): a strip never projects a 2D or 3D pattern, a plane never projects
//! a 3D one. Those pairings are not offered anywhere — there is no picker
//! entry, no wire value and no engine plan for them. A pattern that reaches
//! an engine anyway (an old playlist entry, a shared link, HA) is
//! *incompatible*: it still renders, on the engine's plain fallback
//! coordinates, and [`crate::engine::EffectiveGeometry::compatible`] says so
//! for a UI to flag or hide.
//!
//! The table (§5.4d as amended by #538) — the first option of each cell is
//! the default, and every default reproduces the engine's pre-projection
//! behaviour:
//!
//! | Layout dims | 1D patterns | 2D patterns | 3D patterns |
//! |---|---|---|---|
//! | 1D (strip) | native | — | — |
//! | 2D (matrix / 2D map) | By `index` · Along `x` · Along `y` | native | — |
//! | 3D (lattice / 3D map) | By `index` · Along `x` · `y` · `z` | Repeat along `z` · `y` · `x` | native |
//!
//! Wire form: three fields — `proj1d`, `proj2d`, `proj3d` — one per PATTERN
//! dimensionality, each carrying one of the seven tokens
//! `index|x|y|z|xy|xz|yz`. Only the fields (and values) meaningful for the
//! Layout's dims are in force; a stored value that is not valid for the
//! current pair falls back to that pair's first option, so a device can carry
//! one triple across Layout changes without losing the user's other choices.
//! `proj3d` is never in force (a 3D pattern is native to a 3D Layout and
//! incompatible with any other), and `proj2d` only on a 3D Layout; both stay
//! in the grammar so persisted forms written before #538 keep parsing.
//!
//! A `renderFrame` pattern follows the 2D row when it actually draws in grid
//! space (it names a coordinate/grid-space bulk builtin — the same signal the
//! default-grid rule uses). One that paints only in index space is a strip
//! pattern and stays one, and a whole-frame pattern is never strip-rendered:
//! it owns the buffer.

use core::fmt;
use core::str::FromStr;

/// One projection choice. The same seven tokens serve all three
/// `proj1d`/`proj2d`/`proj3d` fields; which of them are legal depends on the
/// (pattern dims, layout dims) pair — see [`projection_options`].
///
/// The discriminants are the wire/FFI codes and are **stable**: firmware,
/// mirror and wasm all parse the same numbers and the same strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ProjectionMode {
    /// Wiring order — today's fallback everywhere (1D patterns only).
    Index = 0,
    X = 1,
    Y = 2,
    Z = 3,
    /// Reserved. No pair offers a plane mode since #538 (they named the
    /// slice of a 3D pattern shown on a 2D Layout, a pairing that no longer
    /// exists); the codes stay taken so a persisted `proj3d xy` still parses.
    Xy = 4,
    /// Reserved — see [`ProjectionMode::Xy`].
    Xz = 5,
    /// Reserved — see [`ProjectionMode::Xy`].
    Yz = 6,
}

/// Every mode, in wire-code order.
pub const PROJECTION_MODES: [ProjectionMode; 7] = [
    ProjectionMode::Index,
    ProjectionMode::X,
    ProjectionMode::Y,
    ProjectionMode::Z,
    ProjectionMode::Xy,
    ProjectionMode::Xz,
    ProjectionMode::Yz,
];

impl ProjectionMode {
    /// The wire token (`index`, `x`, …). Round-trips through [`FromStr`].
    pub const fn as_str(self) -> &'static str {
        match self {
            ProjectionMode::Index => "index",
            ProjectionMode::X => "x",
            ProjectionMode::Y => "y",
            ProjectionMode::Z => "z",
            ProjectionMode::Xy => "xy",
            ProjectionMode::Xz => "xz",
            ProjectionMode::Yz => "yz",
        }
    }

    /// The stable FFI code (the `#[repr(u8)]` discriminant).
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// The mode for an FFI code, or `None` when the code is unknown.
    pub const fn from_u8(v: u8) -> Option<ProjectionMode> {
        match v {
            0 => Some(ProjectionMode::Index),
            1 => Some(ProjectionMode::X),
            2 => Some(ProjectionMode::Y),
            3 => Some(ProjectionMode::Z),
            4 => Some(ProjectionMode::Xy),
            5 => Some(ProjectionMode::Xz),
            6 => Some(ProjectionMode::Yz),
            _ => None,
        }
    }

    /// The single layout axis this mode names (0 = x, 1 = y, 2 = z), or
    /// `None` for `index` and the plane modes.
    pub const fn axis(self) -> Option<u8> {
        match self {
            ProjectionMode::X => Some(0),
            ProjectionMode::Y => Some(1),
            ProjectionMode::Z => Some(2),
            _ => None,
        }
    }
}

impl fmt::Display for ProjectionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A token that is not one of `index|x|y|z|xy|xz|yz`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseProjectionError;

impl fmt::Display for ParseProjectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected one of index|x|y|z|xy|xz|yz")
    }
}

impl FromStr for ProjectionMode {
    type Err = ParseProjectionError;

    fn from_str(s: &str) -> Result<ProjectionMode, ParseProjectionError> {
        match s {
            "index" => Ok(ProjectionMode::Index),
            "x" => Ok(ProjectionMode::X),
            "y" => Ok(ProjectionMode::Y),
            "z" => Ok(ProjectionMode::Z),
            "xy" => Ok(ProjectionMode::Xy),
            "xz" => Ok(ProjectionMode::Xz),
            "yz" => Ok(ProjectionMode::Yz),
            _ => Err(ParseProjectionError),
        }
    }
}

/// The three defaults a host holds: one choice per PATTERN dimensionality,
/// independent of the Layout (the Layout decides which are in play).
///
/// [`Projection::default`] is the engine's historical behaviour on every
/// pair: by-index for 1D patterns, the xy image for 2D patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Projection {
    /// `proj1d` — how a 1D pattern is shown on a 2D/3D Layout.
    pub proj1d: ProjectionMode,
    /// `proj2d` — how a 2D (or `renderFrame`) pattern is shown on a 3D Layout.
    pub proj2d: ProjectionMode,
    /// `proj3d` — reserved. A 3D pattern is native to a 3D Layout and is
    /// never shown on a smaller one (#538), so this is never in force; it is
    /// kept so a triple stored before #538 round-trips unchanged.
    pub proj3d: ProjectionMode,
}

impl Default for Projection {
    fn default() -> Projection {
        Projection::DEFAULT
    }
}

impl Projection {
    /// By index · repeat along z (= the xy image) · the reserved `xy` — the
    /// first option of every cell of the table, and a no-op on every pair.
    pub const DEFAULT: Projection = Projection {
        proj1d: ProjectionMode::Index,
        proj2d: ProjectionMode::Z,
        proj3d: ProjectionMode::Xy,
    };

    pub const fn new(
        proj1d: ProjectionMode,
        proj2d: ProjectionMode,
        proj3d: ProjectionMode,
    ) -> Projection {
        Projection { proj1d, proj2d, proj3d }
    }

    /// The stored choice for a pattern of `pattern_dims` (0 is read as 1, so
    /// [`crate::engine::Engine::preferred_dims`]'s 0/2/3 can be passed
    /// straight in). Out-of-range dims read as the 1D slot.
    pub const fn get(&self, pattern_dims: u8) -> ProjectionMode {
        match dims(pattern_dims) {
            3 => self.proj3d,
            2 => self.proj2d,
            _ => self.proj1d,
        }
    }

    /// Replace the choice for a pattern of `pattern_dims`.
    pub fn set(&mut self, pattern_dims: u8, mode: ProjectionMode) {
        match dims(pattern_dims) {
            3 => self.proj3d = mode,
            2 => self.proj2d = mode,
            _ => self.proj1d = mode,
        }
    }

    /// The wire field name for a pattern dimensionality (`proj1d`…).
    pub const fn field_name(pattern_dims: u8) -> &'static str {
        match dims(pattern_dims) {
            3 => "proj3d",
            2 => "proj2d",
            _ => "proj1d",
        }
    }

    /// The mode actually in force for this (pattern dims, layout dims) pair:
    /// `None` when there is nothing to project — the pattern is native to
    /// the Layout, or it is incompatible with it (#538) and renders on the
    /// engine's plain fallback coordinates — otherwise the stored choice, or
    /// the pair's first option when the stored choice is not one this pair
    /// offers.
    pub fn effective(&self, pattern_dims: u8, layout_dims: u8) -> Option<ProjectionMode> {
        let opts = projection_options(pattern_dims, layout_dims);
        let first = *opts.first()?;
        let want = self.get(pattern_dims);
        Some(if opts.contains(&want) { want } else { first })
    }
}

/// Normalize a dimensionality: `preferred_dims`' 0 means a strip, i.e. 1;
/// anything above 3 clamps to 3.
pub const fn dims(d: u8) -> u8 {
    match d {
        0 | 1 => 1,
        2 => 2,
        _ => 3,
    }
}

const OPT_1_ON_2: [ProjectionMode; 3] =
    [ProjectionMode::Index, ProjectionMode::X, ProjectionMode::Y];
const OPT_1_ON_3: [ProjectionMode; 4] = [
    ProjectionMode::Index,
    ProjectionMode::X,
    ProjectionMode::Y,
    ProjectionMode::Z,
];
const OPT_2_ON_3: [ProjectionMode; 3] =
    [ProjectionMode::Z, ProjectionMode::Y, ProjectionMode::X];

/// The projection choices that mean anything for a pattern of
/// `pattern_dims` shown on a Layout of `layout_dims` — one row of the §5.4d
/// table, in display order, first = default.
///
/// Empty when there is nothing to project: the pattern is native to the
/// Layout (`pattern_dims == layout_dims`), or the pattern needs more
/// dimensions than the Layout has, which since #538 is never shown
/// ([`compatible`]). UIs build their pickers from this rather than restating
/// the table: a single-option cell is a one-line note, never a disabled
/// control, and an empty one shows nothing at all.
pub fn projection_options(pattern_dims: u8, layout_dims: u8) -> &'static [ProjectionMode] {
    match (dims(pattern_dims), dims(layout_dims)) {
        (1, 2) => &OPT_1_ON_2,
        (1, 3) => &OPT_1_ON_3,
        (2, 3) => &OPT_2_ON_3,
        _ => &[],
    }
}

/// Whether a pattern of `pattern_dims` is one this Layout can show at all
/// (#538): a Layout shows its own dimensionality and lower, never higher.
///
/// A host never *offers* an incompatible pattern, but it can still be
/// handed one — an old playlist entry, a shared link, a Home Assistant
/// call — and the engine renders it rather than going dark
/// ([`crate::engine::EffectiveGeometry::compatible`], `/api/status`
/// `geom.compatible`).
pub const fn compatible(pattern_dims: u8, layout_dims: u8) -> bool {
    dims(pattern_dims) <= dims(layout_dims)
}

/// The human label for one cell of the table (`Along x`, `Repeat along z`,
/// …) — exported so the console, the playground and the CLI all caption a
/// projection the same way. `Native` for a pair with no choice.
pub fn projection_label(
    mode: ProjectionMode,
    pattern_dims: u8,
    layout_dims: u8,
) -> &'static str {
    use ProjectionMode::*;
    match (dims(pattern_dims), dims(layout_dims), mode) {
        (1, 2 | 3, Index) => "By index",
        (1, 2 | 3, X) => "Along x",
        (1, 2 | 3, Y) => "Along y",
        (1, 3, Z) => "Along z",
        (2, 3, X) => "Repeat along x",
        (2, 3, Y) => "Repeat along y",
        (2, 3, Z) => "Repeat along z",
        _ => "Native",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn modes_round_trip_through_strings_and_codes() {
        for m in PROJECTION_MODES {
            assert_eq!(m.as_str().parse::<ProjectionMode>(), Ok(m));
            assert_eq!(ProjectionMode::from_u8(m.as_u8()), Some(m));
            assert_eq!(m.to_string(), m.as_str());
        }
        assert_eq!("".parse::<ProjectionMode>(), Err(ParseProjectionError));
        assert_eq!("XY".parse::<ProjectionMode>(), Err(ParseProjectionError));
        assert_eq!(ProjectionMode::from_u8(7), None);
    }

    #[test]
    fn options_match_the_table() {
        let names = |p, l| {
            projection_options(p, l)
                .iter()
                .map(|m| m.as_str())
                .collect::<alloc::vec::Vec<_>>()
        };
        assert_eq!(names(1, 1), Vec::<&str>::new());
        assert_eq!(names(2, 2), Vec::<&str>::new());
        assert_eq!(names(3, 3), Vec::<&str>::new());
        assert_eq!(names(1, 2), ["index", "x", "y"]);
        assert_eq!(names(1, 3), ["index", "x", "y", "z"]);
        assert_eq!(names(2, 3), ["z", "y", "x"]);
        // preferred_dims()'s 0 is the 1D slot
        assert_eq!(names(0, 2), names(1, 2));
    }

    #[test]
    fn a_layout_never_projects_a_bigger_pattern() {
        // #538: a strip shows no 2D/3D pattern, a plane no 3D pattern —
        // no options, and nothing in force.
        for (p, l) in [(2, 1), (3, 1), (3, 2)] {
            assert!(projection_options(p, l).is_empty(), "{p}D on {l}D");
            assert!(!compatible(p, l), "{p}D on {l}D");
            for m in PROJECTION_MODES {
                let mut proj = Projection::DEFAULT;
                proj.set(p, m);
                assert_eq!(proj.effective(p, l), None, "{p}D on {l}D {m}");
            }
        }
        for (p, l) in [(1, 1), (1, 2), (1, 3), (2, 2), (2, 3), (3, 3)] {
            assert!(compatible(p, l), "{p}D on {l}D");
        }
        // preferred_dims()'s 0 is a 1D pattern: compatible with everything
        assert!(compatible(0, 1));
    }

    #[test]
    fn effective_clamps_to_the_pairs_first_option() {
        let p = Projection::DEFAULT;
        assert_eq!(p.effective(1, 1), None);
        assert_eq!(p.effective(1, 2), Some(ProjectionMode::Index));
        assert_eq!(p.effective(2, 3), Some(ProjectionMode::Z));
        let p = Projection::new(ProjectionMode::Z, ProjectionMode::Y, ProjectionMode::Yz);
        // proj1d=z is meaningless on a 2D layout → first option
        assert_eq!(p.effective(1, 2), Some(ProjectionMode::Index));
        assert_eq!(p.effective(1, 3), Some(ProjectionMode::Z));
        assert_eq!(p.effective(2, 3), Some(ProjectionMode::Y));
        // the reserved plane codes are never in force anywhere
        assert_eq!(p.effective(3, 3), None);
    }

    #[test]
    fn get_set_and_field_names() {
        let mut p = Projection::DEFAULT;
        p.set(0, ProjectionMode::X);
        assert_eq!(p.get(1), ProjectionMode::X);
        p.set(2, ProjectionMode::Y);
        assert_eq!(p.proj2d, ProjectionMode::Y);
        p.set(3, ProjectionMode::Yz);
        assert_eq!(p.get(3), ProjectionMode::Yz);
        assert_eq!(Projection::field_name(0), "proj1d");
        assert_eq!(Projection::field_name(2), "proj2d");
        assert_eq!(Projection::field_name(3), "proj3d");
    }

    #[test]
    fn labels_cover_every_valid_cell() {
        for (p, l) in [(1, 2), (1, 3), (2, 3)] {
            for m in projection_options(p, l) {
                assert_ne!(projection_label(*m, p, l), "Native", "{p}D on {l}D {m}");
            }
        }
        assert_eq!(projection_label(ProjectionMode::X, 2, 2), "Native");
    }
}
