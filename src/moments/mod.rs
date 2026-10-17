//! Playable moments (docs/design/moments.md): small doctrine beats the
//! owner can feel before the slice exists.
//!
//! Layout:
//! - [`registry`]: the one place moments are registered. Authors add a
//!   module here and one push line — nothing else.
//! - [`picker`]: the M-key picker, the active-moment state, the HUD, and
//!   the R-restart / Esc-return rules.
//!
//! The rules stay in `crates/sim`: a moment's [`Moment::setup`] configures
//! the `Sim` through its public API and its [`Moment::status`] reads it.
//! Moment state is a baseline snapshot taken at setup — never ticked.

mod first_molt;
mod forward_siege;
mod picker;
mod registry;
mod rescue;
mod turn_around;

pub use picker::MomentsPlugin;
