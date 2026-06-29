//! Icon-font tokens shared across the UI. PromptFont (loaded in [`crate::menu`]) keeps plain
//! ASCII letters legible but remaps these code points to keyboard / PlayStation-gamepad
//! glyphs — so any text using them must be drawn in that font (tag the entity with
//! [`crate::menu::PromptGlyph`]); in any other font they show up as tofu.
//!
//! Written as `\u{..}` escapes so the source stays pure ASCII and readable in any terminal;
//! the trailing comment names the glyph each one yields.

// PlayStation face buttons (PromptFont keys them off these dashed arrows by direction).
pub(crate) const CROSS: &str = "\u{21E3}"; // down  -> Cross
pub(crate) const SQUARE: &str = "\u{21E0}"; // left  -> Square
pub(crate) const TRIANGLE: &str = "\u{21E1}"; // up    -> Triangle
// Shoulders, d-pad, stick, and the menu buttons.
pub(crate) const L1: &str = "\u{21B0}"; // left shoulder
pub(crate) const R1: &str = "\u{21B1}"; // right shoulder
pub(crate) const DPAD_UD: &str = "\u{21A3}"; // d-pad up/down
pub(crate) const STICK: &str = "\u{21CD}"; // left analog stick
pub(crate) const OPTIONS: &str = "\u{21E8}"; // Options (Start)
pub(crate) const SHARE: &str = "\u{21E6}"; // Share / Create (Select)
// Keyboard keys (single-glyph tokens).
pub(crate) const SPACE: &str = "\u{243A}"; // Space key
pub(crate) const SHIFT: &str = "\u{2429}"; // Shift key
pub(crate) const ESC: &str = "\u{242F}"; // Esc key
