//! The owner's oil painter and watercolour brush (see README.md).
//!
//! * [`oil`]: the oil painter (`Painting`, its tools and strokes, `run_photo` = `crabpaint oilphoto`), the
//!   watercolour / mixed modes, and a pure-Rust port of p5.brush (`brush_fill`, `brush_wash`, `brush_stroke`).
//! * [`spectral`]: Kubelka-Munk pigment mixing (38 bands), after spectral.js as p5.brush ships it.
//!
//! An art tool: the game never links it.

// The owner's code: style lints it was never written against stay off rather than the code being
// reworded (the spectral tables are generated, hence their long digits).
#![allow(
    clippy::excessive_precision,
    clippy::int_plus_one,
    clippy::if_same_then_else,
    clippy::len_zero,
    clippy::new_without_default,
    clippy::type_complexity,
    clippy::needless_range_loop,
    clippy::doc_lazy_continuation,
    clippy::unnecessary_sort_by
)]

pub mod oil;
pub mod spectral;

pub use oil::{Painting, PhotoOptions, Stroke, Tool, brush_fill, brush_stroke, brush_wash, run_mix, run_photo, run_photo_with};
