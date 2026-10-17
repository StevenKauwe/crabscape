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
    /// Extra weight while falling (1 = none). Kept at 1: in water the crab
    /// sinks under the same pull it rose against, so the way down is a sink,
    /// not a drop (the "fall faster" trick belongs to land platformers).
    pub fall_gravity_scale: f32,
    /// Water drag. It grows with the square of the speed, so a hard push-off
    /// is braked quickly and the crab settles into a steady, gentle sink: the
    /// top sinking speed is about `sqrt(net gravity / water_drag)`.
    pub water_drag: f32,

    /// Diving (pushing a way while sinking): sideways speed, as a multiple
    /// of `walk_speed`...
    pub dive_speed: f32,
    /// ...and water drag, as a multiple of `water_drag` (less: it sinks a
    /// little faster, streamlined).
    pub dive_drag: f32,

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
    /// Upward speed of a paddle stroke (made with the flattened fifth legs).
    /// Weaker than the floor push. The first stroke after a jump is the
    /// double jump; more strokes swim higher, one push at a time, sinking in
    /// between.
    pub paddle_speed: f32,
    /// Paddle strokes the crab starts with (the first is the double jump,
    /// then swim strokes) before its legs need a rest on something solid.
    /// None: every stroke is an upgrade, one per egg eaten (see
    /// [`Mover::upgrades`](crate::Mover::upgrades)).
    pub swim_strokes: u32,
    /// The paddle legs' rhythm: the shortest time between two strokes, so
    /// mashing doesn't help.
    pub stroke_interval: f32,
    /// Seconds standing on something solid (floor or rock) that rest the
    /// paddle legs from spent to full. They don't rest while swimming.
    pub stamina_refill_time: f32,
    /// The body's reserve, in strokes: resting refills the paddle legs from
    /// it, and when it runs dry they stay tired. Keeps swimming from being
    /// endless.
    pub stamina_reserve: u32,
    /// Seconds for the reserve to come back by itself from empty to full.
    pub reserve_refill_time: f32,
    /// Top speed of the sprint: holding dodge (on the floor) after the dodge
    /// keeps the crab scuttling fast. Between walking and the dodge burst.
    pub sprint_speed: f32,
    /// What sprinting costs, in reserve strokes per second.
    pub sprint_cost: f32,

    // --- Climbing -------------------------------------------------------
    /// How fast the crab climbs kelp or coral (walking legs: no stamina).
    pub climb_speed: f32,
    /// How close the crab's middle must be to a stalk to grip it, and how far
    /// either side of it the crab can shuffle while holding on.
    pub climb_reach: f32,
    /// Sideways shuffling speed while holding on (a fidget is safe; only
    /// pushing on past the edge of the reach lets go).
    pub climb_shuffle: f32,
    /// The pull-up when climbing past a stalk's top: enough to get over a
    /// rim beside it.
    pub climb_pull_up: f32,
    /// Upward speed of a kick off a stalk (jump + a direction while
    /// climbing). Weaker than a jump off the floor.
    pub climb_leap: f32,
    /// Climbing experience per unit climbed (up or down).
    pub climb_xp_per_unit: f32,
    /// Climbing levels: level n+1 needs n x (n+1) times this much experience
    /// (with 15: 30 for level 2, 90 for 3, 180 for 4, 300 for 5).
    pub climb_level_xp: f32,
    /// Extra climbing speed per Climbing level above 1 (a share of
    /// `climb_speed`).
    pub climb_speed_per_level: f32,

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
    /// How long a flip (dodge in the water) takes.
    pub flip_time: f32,
    /// The dodge goes the way you hold, so it needs a direction. Press dodge
    /// this long before the direction (thumbs and fingers are never quite
    /// together), or before landing or the cooldown ending, and it still
    /// counts.
    pub dodge_buffer: f32,

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

    // --- Moments: molt, biomass and death-moults ------------------------
    /// Stamina gained from eating one carried raw biomass in the field
    /// ("eat raw = stamina now"). Locked while premolt anorexia holds.
    pub raw_eat_stamina: f32,
    /// How long (seconds) the committed premolt runs before ecdysis.
    pub committed_seconds: f32,
    /// How long (seconds) ecdysis lasts: the crab is immobile.
    pub ecdysis_seconds: f32,
    /// How long (seconds) the paper-shell window lasts after ecdysis.
    pub soft_seconds: f32,
    /// Movement scale while soft or in ecdysis recovery (defense ≈ 0 and
    /// slow).
    pub soft_speed_factor: f32,
    /// Ticks a shed claw adds back to the premolt countdown (documented: a
    /// limb lost mid-premolt delays ecdysis so it can regrow fully).
    pub autotomy_delay_ticks: u64,
    /// Seconds before a wild death-moult is eaten by the web (the decay
    /// ring). Home moults never decay.
    pub wild_decay_seconds: f32,
    /// Biomass the molt bill charges for the first feat (the pit's on-ramp
    /// molts are cheaper; bills scale from here).
    pub molt_bill_base: u32,
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
            buoyancy: 0.6, // net pull 960: sinks, gently
            fall_gravity_scale: 1.0,
            water_drag: 0.008, // top sinking speed about 350
            dive_speed: 1.15,
            dive_drag: 0.8, // top diving speed about 390

            jump_speed: 1750.0, // a hard push the water soon brakes: about 190 high
            jump_windup: 0.05,
            jump_carry: 1.25,
            jump_cut: 0.27, // a tap gives a hop of about 60
            coyote_time: 0.10,
            jump_buffer: 0.13,
            paddle_speed: 1050.0, // adds about 140 at the top of a jump
            swim_strokes: 0,
            stroke_interval: 0.30,
            stamina_refill_time: 0.6,
            stamina_reserve: 6,
            reserve_refill_time: 30.0,
            sprint_speed: 480.0,
            sprint_cost: 0.7, // a full reserve lasts about 12 s of sprinting

            climb_speed: 320.0, // the whole pit in about 25 s
            climb_reach: 48.0,
            climb_shuffle: 110.0, // about half a second from the middle to the edge
            climb_pull_up: 700.0,
            climb_leap: 1300.0,      // about two-thirds of a floor jump's height
            climb_xp_per_unit: 0.01, // the whole pit is worth 80
            climb_level_xp: 15.0,    // level 2 at 30, 3 at 90, 4 at 180, 5 at 300
            climb_speed_per_level: 0.08,

            heavy_landing_speed: 290.0, // a full jump lands heavy, a tapped hop (~240) not
            heavy_landing_keep: 0.55,

            dodge_speed: 720.0,
            dodge_time: 0.15,
            dodge_cooldown: 0.45,
            dodge_buffer: 0.10,
            flip_time: 0.45,

            body_half_width: 36.0,
            body_height: 48.0,
            step_up: 14.0,
            ground_snap: 12.0,

            raw_eat_stamina: 1.5,
            committed_seconds: 2.0,
            ecdysis_seconds: 1.0,
            soft_seconds: 5.0,
            soft_speed_factor: 0.45,
            autotomy_delay_ticks: 360, // 3 s: the siege verb
            wild_decay_seconds: 30.0,  // the web is coming
            molt_bill_base: 3,
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
