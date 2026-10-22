//! The oil painter, from brushwork (https://github.com/StevenKauwe/brushwork, MIT), which is upstream: fixes to
//! the painting itself go there. This crate keeps CrabScape's side: the `crabpaint` command line, and the paths
//! CrabScape has always used.
//!
//! * [`oil`]: the painter (`Painting`, its tools and strokes, `run_photo_with` = `crabpaint oilphoto`, the
//!   watercolour / mixed modes, the p5.brush port `brush_fill` / `brush_wash` / `brush_stroke`).
//! * [`spectral`]: Kubelka-Munk pigment mixing (38 bands), after spectral.js.
//!
//! An art tool: the game never links it.

/// brushwork's painter, under the name CrabScape uses.
pub mod oil {
    pub use brushwork::*;
}
pub use brushwork_kit::spectral;

pub use brushwork::{Painting, PhotoOptions, Stroke, Tool, brush_fill, brush_stroke, brush_wash, run_mix, run_photo, run_photo_with};
