//! The one place moments are registered.
//!
//! Moment authors: write `src/moments/<name>.rs` implementing
//! `crabsim::Moment`, then add two lines here:
//!
//! ```ignore
//! mod <name>;
//! ...
//! all.push(Box::new(<name>::<Name>));
//! ```
//!
//! Everything else (picker, HUD, restart rules, telemetry) is the
//! framework's job.

use crabsim::Moment;

/// Every registered moment, in picker order.
pub fn all() -> Vec<Box<dyn Moment>> {
    let all: Vec<Box<dyn Moment>> = Vec::new();
    // (moment authors: one push line each, in the order they should list)
    all
}
