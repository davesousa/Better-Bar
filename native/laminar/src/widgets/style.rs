//! Design tokens for widgets (docs/widget-design.md §2): card shapes, margins, radii, the
//! type scale, badge and control sizes, list pitches and chart settings. Logical px.
//!
//! Every layout should take its numbers from here, so the families stay consistent and a
//! change to the system is one edit. Colours live on `Canvas` (they depend on the theme).

// Families adopt these one by one; until then some are unused.
#![allow(dead_code)]

// ------------------------------------------------------------------ shapes and radii

/// Launchpad card corner radius (Small, Medium, Large). The card shadow uses it too.
pub const CARD_R: f32 = 18.0;
/// Dock pill corner radius (Mini, Dock M, Dock L).
pub const DOCK_R: f32 = 12.0;

/// Outer margins, measured to the ink.
pub const M_MINI: f32 = 7.0;
pub const M_DOCK: f32 = 12.0;
pub const M_SMALL: f32 = 14.0;
pub const M_CARD: f32 = 16.0;

/// The spacing scale: no other gaps.
pub const SP: [f32; 9] = [2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0, 24.0];

/// Smallest radius any inner panel gets.
pub const R_MIN: f32 = 6.0;

/// Concentric rule: a panel inset `inset` from a card with radius `outer` gets this radius.
pub fn concentric(outer: f32, inset: f32) -> f32 {
    (outer - inset).max(R_MIN)
}

// ------------------------------------------------------------------ type scale

/// A type role: size and weight. Colour comes from the theme (text1/2/3 or an accent).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Type {
    pub size: f32,
    pub weight: u32,
    /// The smallest size `fit` may shrink this role to.
    pub min: f32,
}

const fn ty(size: f32, weight: u32, min: f32) -> Type {
    Type { size, weight, min }
}

/// Large-only heroes and clocks (time/temperature heroes pass weight 300).
pub const HERO_XL: Type = ty(44.0, 600, 28.0);
/// Medium/Large key value.
pub const HERO: Type = ty(32.0, 600, 22.0);
/// Small key value.
pub const HERO_S: Type = ty(26.0, 600, 18.0);
/// Dock value, chart-header value, secondary stat values.
pub const VALUE: Type = ty(17.0, 600, 13.0);
/// Header-row title, list emphasis.
pub const TITLE: Type = ty(13.0, 600, 11.0);
/// List row labels.
pub const BODY: Type = ty(13.0, 400, 11.0);
/// Line under a hero; Small caption.
pub const CAPTION: Type = ty(12.0, 400, 10.0);
/// Trailing row values, dock caption, meter labels (500 for trailing values).
pub const FOOT: Type = ty(11.0, 400, 10.0);
/// Axis ticks, weekday letters, "PM", mode labels.
pub const MICRO: Type = ty(10.0, 600, 10.0);
/// The one value on a Mini dock widget.
pub const MINI_VALUE: Type = ty(14.0, 600, 11.0);

/// Time and temperature heroes: weight by size (never 200).
pub fn time_weight(size: f32) -> u32 {
    if size >= 32.0 {
        300
    } else if size >= 20.0 {
        400
    } else {
        600
    }
}

// ------------------------------------------------------------------ badges and controls

pub const BADGE_MINI: f32 = 20.0;
pub const BADGE_MINI_VIS: f32 = 38.0;
pub const BADGE_DOCK: f32 = 30.0;
pub const BADGE_SMALL: f32 = 28.0;
pub const BADGE_GLANCE: f32 = 24.0;
pub const BADGE_HEADER: f32 = 20.0;
/// Badge corner radius as a fraction of its size.
pub const BADGE_R: f32 = 0.25;

pub const BTN_PRIMARY: f32 = 36.0;
pub const BTN_SECONDARY: f32 = 28.0;
pub const BTN_PRIMARY_SMALL: f32 = 28.0;
pub const BTN_SECONDARY_SMALL: f32 = 24.0;
pub const BTN_PRIMARY_DOCK: f32 = 30.0;
pub const BTN_SECONDARY_DOCK: f32 = 26.0;
/// A glyph inside a button or plate is this fraction of its diameter.
pub const GLYPH_IN: f32 = 0.45;
/// Smallest hit target.
pub const HIT_MIN: f32 = 28.0;

// ------------------------------------------------------------------ lists

pub const ROW_L: f32 = 24.0;
pub const ROW_M: f32 = 20.0;
pub const ROW_S: f32 = 18.0;
pub const ROWS_MAX_S: usize = 3;
pub const ROWS_MAX_M: usize = 2;
pub const ROWS_MAX_L: usize = 5;

// ------------------------------------------------------------------ charts, rings, meters

pub const STROKE: f32 = 2.0;
pub const STROKE_SPARK: f32 = 1.5;
/// Area fill opacity at the top of the gradient.
pub const FILL_DARK: f32 = 0.30;
pub const FILL_LIGHT: f32 = 0.22;
/// Chart headroom: data min − 8% … max + 14% of the range.
pub const CHART_PAD_LO: f64 = 0.08;
pub const CHART_PAD_HI: f64 = 0.14;
pub const CHART_DOT: f32 = 3.0;

pub const METER_H: f32 = 6.0;
pub const METER_H_COMPACT: f32 = 4.0;
/// Ring thickness as a fraction of its diameter (at least 3).
pub const RING_T: f32 = 0.11;

pub const HEAT_GAP: f32 = 2.0;
pub const HEAT_STEPS: [f32; 4] = [0.35, 0.55, 0.75, 1.0];

// ------------------------------------------------------------------ template lines

/// Large Glance faces: separator under the medium block, then the detail region.
pub const SEP_Y_LARGE: f32 = 96.0;
pub const DETAIL_TOP: f32 = 108.0;
/// Header template: baseline of the title row, and where content starts under it.
pub const HEADER_BASE: f32 = 31.0;
pub const CONTENT_TOP: f32 = 44.0;
