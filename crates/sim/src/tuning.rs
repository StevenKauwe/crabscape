//! Every number that decides how the crab *moves*, in one place.
//!
//! Distances are in world units. The crab is about 120 units wide (claw tip
//! to claw tip) and its shell about 75, so "100 units" is "a bit less than
//! one crab". Times are in seconds, speeds in units per second, and
//! accelerations in units per second per second.
//!
//! Want to experiment? Change a number here and run the game again. The
//! tests (`physics.rs` and `tests/scenarios.rs`) check a few promises (a full
//! jump is about 1.6 crabs high, the test level's gap needs the paddle double
//! jump, and so on), so `cargo test` tells you if a change breaks the level.
//! `cargo crabsim` shows the numbers of a scripted run before and after.
//!
//! (The camera and lantern numbers only change how things look, so they live
//! with the game, in `src/tuning.rs`.)

/// The feel of the crab's movement. In the game it is a Bevy *resource* (one
/// shared copy), inside the [`Sim`](crate::Sim).
#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    // --- Walking --------------------------------------------------------
    /// Top walking speed.
    pub walk_speed: f32,
    /// How quickly the crab gets up to speed on the ground. Heavy, but it
    /// reaches full speed in about a seventh of a second, so it never feels
    /// like pushing a fridge.
    pub ground_accel: f32,
    /// How quickly it stops when you let go.
    pub ground_decel: f32,
    /// How quickly it stops and reverses when you push the other way. Bigger
    /// than `ground_accel` so turning around feels snappy.
    pub ground_turn: f32,
    /// How quickly it sheds speed above `walk_speed` (after a dodge). Low, so
    /// the burst carries on a little: that's momentum.
    pub overspeed_decel: f32,
    /// Steering in the water (while jumping or falling). Less grip than on
    /// the sea floor.
    pub air_accel: f32,
    /// Slowing down in the water when you let go.
    pub air_decel: f32,

    // --- Sinking --------------------------------------------------------
    /// Pull of gravity on a crab in air.
    pub gravity: f32,
    /// How much of `gravity` the water's buoyancy cancels (0 = none, 1 = the
    /// crab floats). A crab is denser than water, so it still sinks.
    pub buoyancy: f32,
    /// Extra weight while falling (1 = none). Falls feel heavier than rises.
    pub fall_gravity_scale: f32,
    /// Water drag. It grows with the square of the speed, so slow motion is
    /// barely touched but fast falls are capped: the top falling speed is
    /// about `sqrt(net gravity / water_drag)`.
    pub water_drag: f32,

    // --- Jumping --------------------------------------------------------
    /// Upward speed of the push off the floor. Strong!
    pub jump_speed: f32,
    /// The crouch before take-off (anticipation). Short enough that you
    /// don't feel a delay; long enough to see the crab load its legs.
    pub jump_windup: f32,
    /// The most sideways speed a jump keeps, as a multiple of `walk_speed`.
    /// Jumping out of a dodge carries a little extra, but not so much that
    /// a dodge-jump clears the gap meant for the paddle stroke.
    pub jump_carry: f32,
    /// Let go of jump while rising and the upward speed is multiplied by
    /// this, so a tap gives a hop and holding gives a full jump.
    pub jump_cut: f32,
    /// After walking off a ledge you can still jump for this long
    /// ("coyote time"). Heavy characters need it to feel fair.
    pub coyote_time: f32,
    /// Press jump this long before landing and it still counts
    /// ("jump buffering").
    pub jump_buffer: f32,
    /// Upward speed of the paddle stroke (the double jump, made with the
    /// flattened fifth legs). Weaker than the floor push.
    pub paddle_speed: f32,
    /// Paddle strokes allowed per jump.
    pub paddles_per_jump: u32,

    // --- Landing --------------------------------------------------------
    /// Landing faster than this counts as a heavy landing: big squash, a
    /// puff of sand, and some sideways speed lost.
    pub heavy_landing_speed: f32,
    /// Fraction of sideways speed kept after a heavy landing.
    pub heavy_landing_keep: f32,

    // --- Dodge ----------------------------------------------------------
    /// Speed of the quick sideways scuttle.
    pub dodge_speed: f32,
    /// How long the scuttle lasts.
    pub dodge_time: f32,
    /// Time before you can dodge again (counted from the start).
    pub dodge_cooldown: f32,

    // --- Body and collisions -------------------------------------------
    /// Half the width of the crab's collision box (its shell, not the legs:
    /// letting the legs overhang an edge looks right for a crab).
    pub body_half_width: f32,
    /// Height of the collision box.
    pub body_height: f32,
    /// The crab walks up steps lower than this without jumping.
    pub step_up: f32,
    /// While walking, stay glued to a floor that drops away by less than
    /// this (so walking down a slope doesn't turn into tiny falls).
    pub ground_snap: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            walk_speed: 270.0,
            ground_accel: 1900.0,
            ground_decel: 2600.0,
            ground_turn: 3400.0,
            overspeed_decel: 1300.0,
            air_accel: 1100.0,
            air_decel: 500.0,

            gravity: 2400.0,
            buoyancy: 0.4, // net pull 1440: sinks, but slower than in air
            fall_gravity_scale: 1.25,
            water_drag: 0.0020, // top falling speed about 950

            jump_speed: 920.0, // about 190 units high (1.6 crabs) with drag
            jump_windup: 0.05,
            jump_carry: 1.25,
            jump_cut: 0.5, // a tap gives a hop of about 60
            coyote_time: 0.10,
            jump_buffer: 0.13,
            paddle_speed: 740.0, // adds about 140 at the top of a jump
            paddles_per_jump: 1,

            heavy_landing_speed: 520.0,
            heavy_landing_keep: 0.55,

            dodge_speed: 720.0,
            dodge_time: 0.15,
            dodge_cooldown: 0.45,

            body_half_width: 36.0,
            body_height: 48.0,
            step_up: 14.0,
            ground_snap: 12.0,
        }
    }
}

impl Tuning {
    /// A short code that changes whenever any number here changes. Replays
    /// store it, so a replay made with different tuning can warn you.
    pub fn fingerprint(&self) -> u64 {
        // `{:?}` prints every field, and prints each `f32` exactly.
        crate::hash::fnv1a(format!("{self:?}").as_bytes())
    }

    /// The downward pull in the water, after buoyancy.
    pub fn net_gravity(&self) -> f32 {
        self.gravity * (1.0 - self.buoyancy)
    }
}
