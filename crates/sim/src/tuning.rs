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
//!
//! Fighting numbers are in *ticks* (120 = 1 s), as in the combat spec
//! (`docs/design/creatures/README.md`), so a window can be tried tick by tick.

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

    // --- Fighting (ticks) ----------------------------------------------
    /// The crab's shell (health) and poise, and how they recover.
    pub crab: VitalsTuning,
    /// After a hit that doesn't break poise, this many ticks with no walking
    /// control, so the knockback reads (short: a tank shrugs it off).
    pub hit_stun_ticks: u32,
    /// Ticks after health runs out before the crab wakes in its home shell
    /// (leaving its cast-off shell where it fell, `world.md`).
    pub respawn_ticks: u32,
    /// Home mends the shell only after this long unhit (a rest, not a
    /// refuge mid-fight).
    pub home_heal_after_ticks: u32,
    /// How long the crab takes to back into its home shell, and to come back
    /// out (seconds; see [`crate::home`]). Out is quicker: crabs dart out.
    pub home_in_time: f32,
    pub home_out_time: f32,
    /// The crusher's smash: slow, heavy, committed.
    pub crusher: ClawTuning,
    /// The cutter's jab: quick and light.
    pub cutter: ClawTuning,
    /// Press a claw this many ticks before the current swing ends and it
    /// still comes out.
    pub swing_buffer_ticks: u32,
    /// Walking speed during a swing's recovery, as a share of normal (the
    /// wind-up and the blow itself plant the feet: no walking).
    pub recovery_walk: f32,
    /// Turning round: the crab faces the camera, crusher on its right
    /// (screen-left), cutter on its left, and never swaps them. A blow towards
    /// the side its claw isn't on turns the body round first (showing its
    /// back, or its front again): this many ticks, added to the wind-up
    /// (0 = no turning).
    pub turn_ticks: u32,
    /// A turn into a blow is a pivoting sidestep: it carries the crab this
    /// far towards the blow (units), and the momentum runs on into it.
    pub turn_travel: f32,
    /// Shown its back, it turns to face the camera again after this many
    /// ticks out of a fight (no swing, guard or crush).
    pub turn_back_after: u32,
    /// Momentum: a swing started at speed hits harder and lunges further,
    /// by up to this share at sprinting speed (0 = speed doesn't matter).
    pub momentum: f32,
    /// The crusher charges: hold smash to keep winding up for up to this
    /// many more ticks, let go to strike (0 = no charging: a fixed wind-up).
    pub charge_ticks: u32,
    /// A full charge does this much more (damage, poise, knockback, lunge).
    pub charge_bonus: f32,
    /// Ticks of hit-stop when a counter smash lands (the payoff beat).
    pub counter_hitstop_ticks: u32,
    /// A counter smash's wind-up is this many ticks shorter.
    pub counter_haste: u32,
    /// A counter smash that lands seizes the target in the crusher and
    /// crushes it (None = a plain hit).
    pub crush: Option<CrushTuning>,
    /// How quickly an enemy sheds the knockback of a blow (units per second
    /// per second): high = a hard shove that stops, low = a slide.
    pub knock_decel: f32,

    // --- Defence (ticks) -------------------------------------------------
    /// Share of a blow's damage that raised claws let through (poise damage
    /// is taken in full).
    pub block_damage: f32,
    /// The same with the guard cracked (an armour breaker's blow).
    pub cracked_block_damage: f32,
    /// Walking speed with the claws up, as a share of normal.
    pub block_walk: f32,
    /// Share of a blow's knockback a block still takes.
    pub block_knockback: f32,
    pub block_hitstop_ticks: u32,
    /// A block pressed at most this many ticks before a blow lands parries it.
    pub parry_window_ticks: u32,
    /// After a press, a new press within this many ticks doesn't open a new
    /// window (so mashing block isn't a parry). A parry clears it.
    pub parry_lockout_ticks: u32,
    pub parry_hitstop_ticks: u32,
    /// After a parry, a smash started within this many ticks is a counter...
    pub counter_ticks: u32,
    /// ...and does this many times the damage and poise damage.
    pub counter_damage: f32,
    /// The dodge's (and the flip's) invulnerable ticks, counting its first
    /// tick as 1.
    pub dodge_invulnerable: (u32, u32),

    // --- Enemies ----------------------------------------------------------
    /// A plain test enemy (tell, strike, recover). Tests and crabsim only:
    /// never placed in a level.
    pub fixture: EnemyTuning,
    /// The peacock mantis shrimp: a rusher and armour breaker
    /// (`docs/design/creatures/mantis-shrimp.md`, its "Implementation spec").
    pub mantis: EnemyTuning,
    /// A snapping shrimp: a turret at its burrow whose snap sends a shock
    /// along the floor: jump or dodge it, it can't be parried
    /// (`creatures/snapping-shrimp.md`, #69).
    pub snapper: EnemyTuning,
    /// A snowflake moray: an ambusher in a crevice that smells you coming,
    /// coils into an S and lunges, and holds on (`creatures/snowflake-moray.md`,
    /// #71).
    pub moray: EnemyTuning,
    /// What a crab sees and remembers (`crate::memory`).
    pub memory: MemoryTuning,
}

/// One kind of enemy: its body, senses, state timings and attack. Times in
/// ticks. See `docs/design/creatures/`.
#[derive(Clone, Debug, PartialEq)]
pub struct EnemyTuning {
    pub vitals: VitalsTuning,
    /// Hurtbox: width and height, standing on its feet point.
    pub body: (f32, f32),
    /// The front this long is the head: hits there count `head_recover`
    /// times while it recovers (head low)...
    pub head: f32,
    pub head_recover: f32,
    /// ...and the back this long is the tail, hits there count `tail` times.
    pub tail: f32,
    pub tail_damage: f32,
    /// Waits hidden in a burrow at home (can't be hit) until it notices.
    pub burrows: bool,
    /// Notices a crab this close (sideways, in units).
    pub notice_range: f32,
    /// Ticks from noticing to coming at it (emerging from the burrow).
    pub notice_ticks: u32,
    /// Gives up on a crab further than this from home.
    pub give_up_range: f32,
    pub approach_speed: f32,
    /// Starts its tell when the crab's middle is this close to its head.
    pub strike_range: f32,
    /// A pause, sizing the crab up, between attacks: random in this range
    /// (the sim's seeded generator).
    pub pause_ticks: (u32, u32),
    /// The whole tell...
    pub tell_ticks: u32,
    /// ...ending with this long cocked and still: the warning beat.
    pub cocked_ticks: u32,
    /// A shorter tell before each extra strike of a combo.
    pub recock_ticks: u32,
    /// Extra strikes, only after one landed.
    pub combo_extra: u32,
    pub active_ticks: u32,
    pub recover_ticks: u32,
    /// Stagger after being parried.
    pub parried_ticks: u32,
    /// Notices a crab only if it moves faster than this (units per second;
    /// feeling the vibration of its steps); 0 = sees it standing still too.
    pub notice_moving: f32,
    /// Notices by smell: a crab must stay in range this long first (ticks;
    /// light makes no difference). 0 = at once (sight).
    pub scent_ticks: u32,
    /// Below this share of its health it flees home (0 = never)...
    pub flee_below: f32,
    pub flee_speed: f32,
    /// ...and stays hidden this long.
    pub hide_ticks: u32,
    /// After its health runs out, another one moves in this much later (0 =
    /// never).
    pub respawn_ticks: u32,
    pub strike: StrikeTuning,
}

/// What a crab perceives and how it remembers (`crate::memory`).
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryTuning {
    /// Memory grid cell size (units).
    pub cell: f32,
    /// Memory steps this often (ticks).
    pub step_ticks: u32,
    /// Sight: half-width of the ellipse it perceives (units)...
    pub sight: f32,
    /// ...its half-height as a share of that...
    pub sight_tall: f32,
    /// ...and how much wider per culture in the lantern.
    pub sight_per_culture: f32,
    /// Out of sight this many steps, a place seen again is one more visit.
    pub visit_gap_steps: u32,
    /// How well a place is known after 1, 2 and 3+ visits (0..1).
    pub learned: [f32; 3],
    /// The share of that kept however long ago it was seen.
    pub kept: f32,
    /// The rest halves over this many steps, times the visits.
    pub half_life_steps: u32,
}

/// An enemy's blow: a circle ahead of its head.
#[derive(Clone, Debug, PartialEq)]
pub struct StrikeTuning {
    pub damage: f32,
    pub poise: f32,
    /// Centre of the hit circle: this far ahead of the head...
    pub ahead: f32,
    /// ...this high above its feet...
    pub height: f32,
    /// ...this big.
    pub radius: f32,
    pub knockback: (f32, f32),
    pub hitstop_ticks: u32,
    pub parryable: bool,
    pub blockable: bool,
    /// Share of damage a block lets through, if not the usual.
    pub block_damage: Option<f32>,
    /// Blocking it cracks the crab's guard this long.
    pub armour_break_ticks: u32,
    /// A second hit this many ticks after it connects (a cavitation bubble):
    /// ignores block, cancelled by a parry.
    pub follow_up: Option<FollowUp>,
    /// Not a circle but a shock along the floor: this long from the head,
    /// `height` tall from the floor (a jump clears it). 0 = the circle.
    pub floor_len: f32,
    /// The circle travels this far out from `ahead` over the active ticks
    /// (a lunge from a crevice; the body stays put).
    pub lunge: f32,
    /// An unblocked hit seizes the crab (a bite that holds on).
    pub grab: Option<GrabTuning>,
}

/// A bite that holds on: the crab can't act until it struggles free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrabTuning {
    /// Longest hold (ticks), then it lets go...
    pub ticks: u32,
    /// ...each dodge press takes this many ticks off...
    pub struggle: u32,
    /// ...and every this many ticks held, this much damage.
    pub every: u32,
    pub damage: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FollowUp {
    pub delay_ticks: u32,
    pub damage: f32,
    pub poise: f32,
}

/// The crusher's crush: a counter smash clamps on, squeezes (cracks), then
/// lets go with the counter's full blow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrushTuning {
    /// How long it holds on (ticks), the cracks spread evenly over it.
    pub ticks: u32,
    pub cracks: u32,
    /// Each crack's damage (on top of the release's) and its freeze.
    pub crack_damage: f32,
    pub crack_hitstop: u32,
}

/// Health ("shell") and poise for anything that fights, crab or enemy: the
/// same rules for both.
#[derive(Clone, Debug, PartialEq)]
pub struct VitalsTuning {
    pub health: f32,
    /// Each hit's poise damage drains it; at 0 the fighter staggers.
    pub poise: f32,
    /// Poise refilled per second, once unhit for `poise_delay_ticks`.
    pub poise_refill: f32,
    pub poise_delay_ticks: u32,
    /// How long a poise break staggers (can't act). Poise is full after.
    pub stagger_ticks: u32,
}

/// One claw strike: wind-up, active (the hitbox is out), recovery.
#[derive(Clone, Debug, PartialEq)]
pub struct ClawTuning {
    pub windup_ticks: u32,
    pub active_ticks: u32,
    pub recovery_ticks: u32,
    pub damage: f32,
    pub poise: f32,
    /// The hitbox: a box this wide beside the body on the facing side...
    pub reach: f32,
    /// ...and this tall, centred on the body's middle height.
    pub height: f32,
    /// Push given to whatever it hits (x away from the crab, y up).
    pub knockback: (f32, f32),
    /// Hit-stop: both fighters freeze this many ticks when it connects.
    pub hitstop_ticks: u32,
    /// The body goes with the blow: forward speed given on its first active
    /// tick (0 = none). The feet then brake it as usual.
    pub lunge: f32,
    /// From this tick of the recovery on, a claw press cuts the recovery
    /// short and swings again (None = only when the swing is over). Dodge
    /// and block always cut a recovery short.
    pub chain_after: Option<u32>,
    /// On a hit, the crab bounces back off what it struck (sideways speed
    /// away from it; 0 = none).
    pub recoil: f32,
    /// On a hit, the crab hops up off it (upward speed; 0 = none).
    pub hop: f32,
}

impl ClawTuning {
    /// A plain strike: no lunge, no chaining, no charging.
    pub const fn plain(
        (windup_ticks, active_ticks, recovery_ticks): (u32, u32, u32),
        damage: f32,
        poise: f32,
        reach: f32,
        height: f32,
        knockback: (f32, f32),
        hitstop_ticks: u32,
    ) -> Self {
        Self {
            windup_ticks,
            active_ticks,
            recovery_ticks,
            damage,
            poise,
            reach,
            height,
            knockback,
            hitstop_ticks,
            lunge: 0.0,
            chain_after: None,
            recoil: 0.0,
            hop: 0.0,
        }
    }
}

impl Default for Tuning {
    /// The game's combat: the base numbers with the chosen feel preset
    /// (G, `crate::feel`) laid over them.
    fn default() -> Self {
        let mut t = Self::base();
        crate::feel::Feel::default().spec().apply(&mut t);
        t
    }
}

impl Tuning {
    /// The numbers before any feel preset: what [`crate::feel::Feel`]s are
    /// laid over (preset A's swing numbers).
    pub fn base() -> Self {
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

            crab: VitalsTuning {
                health: 100.0,
                poise: 100.0,
                poise_refill: 40.0,
                poise_delay_ticks: 60,
                stagger_ticks: 40,
            },
            hit_stun_ticks: 10,
            respawn_ticks: 150, // 1.25 s, back near the fight
            home_heal_after_ticks: 240,
            home_in_time: 0.45,
            home_out_time: 0.25,
            // Hit-stop 7: about 60 ms, you feel it land. Reach: the claw's
            // length past the shell (the longer chelipeds, #90).
            crusher: ClawTuning::plain((14, 6, 22), 12.0, 25.0, 44.0, 30.0, (320.0, 0.0), 7),
            // A touch shorter than the crusher (tuned).
            cutter: ClawTuning::plain((6, 4, 12), 5.0, 8.0, 38.0, 24.0, (90.0, 0.0), 3),
            swing_buffer_ticks: 12,
            recovery_walk: 0.35,
            turn_ticks: 0,
            turn_travel: 0.0,
            turn_back_after: 240,
            momentum: 0.0,
            charge_ticks: 0,
            charge_bonus: 0.0,
            counter_hitstop_ticks: 7,
            counter_haste: 0,
            knock_decel: 1500.0,
            crush: None,

            block_damage: 0.2,
            cracked_block_damage: 0.4,
            block_walk: 0.4,
            block_knockback: 0.5,
            block_hitstop_ticks: 5,
            parry_window_ticks: 10,
            parry_lockout_ticks: 24, // 0.2 s: mashing faster than 5 a second opens no window
            parry_hitstop_ticks: 9,
            counter_ticks: 45,
            counter_damage: 2.0,
            dodge_invulnerable: (2, 13),

            fixture: EnemyTuning {
                vitals: VitalsTuning {
                    health: 40.0,
                    poise: 30.0,
                    poise_refill: 30.0,
                    poise_delay_ticks: 60,
                    stagger_ticks: 40,
                },
                body: (60.0, 30.0),
                head: 0.0,
                head_recover: 1.0,
                tail: 0.0,
                tail_damage: 1.0,
                burrows: false,
                notice_range: 400.0,
                notice_ticks: 1,
                give_up_range: 2000.0,
                approach_speed: 150.0,
                strike_range: 60.0,
                pause_ticks: (0, 0),
                tell_ticks: 30,
                cocked_ticks: 10,
                recock_ticks: 20,
                combo_extra: 0,
                active_ticks: 6,
                recover_ticks: 40,
                parried_ticks: 50,
                flee_below: 0.0,
                flee_speed: 0.0,
                hide_ticks: 0,
                respawn_ticks: 0,
                notice_moving: 0.0,
                scent_ticks: 0,
                strike: StrikeTuning {
                    damage: 10.0,
                    poise: 20.0,
                    ahead: 30.0,
                    height: 15.0,
                    radius: 14.0,
                    knockback: (200.0, 0.0),
                    hitstop_ticks: 6,
                    parryable: true,
                    blockable: true,
                    block_damage: None,
                    armour_break_ticks: 0,
                    follow_up: None,
                    floor_len: 0.0,
                    lunge: 0.0,
                    grab: None,
                },
            },

            // "Real" = from the page's sources; everything else is tuned.
            mantis: EnemyTuning {
                vitals: VitalsTuning {
                    health: 60.0,
                    poise: 50.0,
                    poise_refill: 25.0,
                    poise_delay_ticks: 60,
                    stagger_ticks: 50,
                },
                body: (94.0, 22.0), // 15 cm long (real: 3-18 cm), slim
                head: 24.0,
                head_recover: 1.5, // head low after a strike
                tail: 16.0,
                tail_damage: 0.5,    // the heavily armoured telson (real)
                burrows: true,       // U-shaped burrows (real)
                notice_range: 300.0, // a sight hunter; in light (the dark rule is #68)
                notice_ticks: 24,    // emerging
                give_up_range: 500.0,
                approach_speed: 260.0, // darts, faster than the crab walks
                strike_range: 70.0,
                pause_ticks: (20, 50),
                tell_ticks: 78,   // rumble and rear (48), then the cocked hold (30)
                cocked_ticks: 30, // the latch is set: parry at its end
                recock_ticks: 24,
                // Strikes the shell again and again (real); two strikes for
                // now, so an undefended combo (2 x 36) can't kill a full
                // shell. Brutality comes later from located body damage.
                combo_extra: 1,
                active_ticks: 3, // real: a few ms, under one tick
                recover_ticks: 60,
                parried_ticks: 50,
                // Retreats to its burrow (real), but only once a fight, badly
                // hurt, and not for long: it peeks out and comes back.
                flee_below: 0.25,
                flee_speed: 260.0,
                hide_ticks: 300, // 2.5 s in the burrow
                respawn_ticks: 2400,
                notice_moving: 0.0,
                scent_ticks: 0,
                strike: StrikeTuning {
                    damage: 28.0,
                    poise: 60.0,
                    ahead: 40.0,
                    height: 11.0,
                    radius: 12.0,
                    knockback: (300.0, 0.0),
                    hitstop_ticks: 12, // the loudest crack on the reef
                    parryable: true,
                    blockable: true,
                    block_damage: Some(0.0), // the club is stopped...
                    armour_break_ticks: 600, // ...but the guard cracks for 5 s
                    // The cavitation bubble: 0.4-0.5 ms after impact (real).
                    follow_up: Some(FollowUp {
                        delay_ticks: 1,
                        damage: 8.0,
                        poise: 0.0,
                    }),
                    floor_len: 0.0,
                    lunge: 0.0,
                    grab: None,
                },
            },

            // "Real" = from the page's sources; everything else is tuned.
            snapper: EnemyTuning {
                vitals: VitalsTuning {
                    health: 12.0,
                    poise: 10.0,
                    poise_refill: 20.0,
                    poise_delay_ticks: 60,
                    stagger_ticks: 30, // stunned
                },
                body: (34.0, 12.0), // about 5 cm (real: 3-5)
                head: 0.0,
                head_recover: 1.0,
                tail: 0.0,
                tail_damage: 1.0,
                burrows: true,       // lives in a burrow (real)
                notice_range: 120.0, // feels the vibration of steps (real: mechanosensory)
                notice_ticks: 12,
                give_up_range: 200.0,
                approach_speed: 0.0, // a turret: it guards its burrow mouth
                strike_range: 72.0,  // its shock (38) reaches a crab this close
                pause_ticks: (10, 30),
                tell_ticks: 54, // real about 60 (500 ms), a little shorter
                cocked_ticks: 30,
                recock_ticks: 0,
                combo_extra: 0,
                active_ticks: 2,   // real: under 1 ms
                recover_ticks: 72, // re-cocking: the punish window
                parried_ticks: 0,
                flee_below: 0.99, // into its burrow when hit
                flee_speed: 200.0,
                hide_ticks: 360,
                respawn_ticks: 2400,
                notice_moving: 135.0, // half walking speed: creep past it
                scent_ticks: 0,
                strike: StrikeTuning {
                    damage: 6.0,
                    poise: 45.0,
                    ahead: 0.0,
                    height: 30.0, // hugs the floor: a jump clears it
                    radius: 0.0,
                    knockback: (220.0, 0.0),
                    hitstop_ticks: 6,
                    parryable: false, // a shock in the water, not a blow
                    blockable: true,
                    block_damage: None,
                    armour_break_ticks: 0,
                    follow_up: None,
                    floor_len: 38.0, // 6 cm, tuned up from the real ~1 cm
                    lunge: 0.0,
                    grab: None,
                },
            },
            moray: EnemyTuning {
                vitals: VitalsTuning {
                    health: 90.0,
                    poise: 70.0,
                    poise_refill: 25.0,
                    poise_delay_ticks: 60,
                    stagger_ticks: 60,
                },
                // The head out of the crevice (the rock shields the rest) of
                // a 60 cm adult: 7 cm long (not verified), 3.8 cm deep (real:
                // depth at the gill opening is a 14th to a 23rd of its length).
                body: (44.0, 24.0),
                head: 0.0,
                head_recover: 1.5, // the neck stretched out after a lunge
                tail: 0.0,
                tail_damage: 1.0,
                burrows: true, // rests in a crevice, head out (real)
                notice_range: 260.0,
                notice_ticks: 18,
                give_up_range: 320.0,
                approach_speed: 0.0, // strikes from its crevice
                strike_range: 150.0,
                pause_ticks: (30, 60),
                tell_ticks: 60, // pulls back into an S, mouth shut
                cocked_ticks: 24,
                recock_ticks: 0,
                combo_extra: 0,
                active_ticks: 8,
                recover_ticks: 50,
                parried_ticks: 60,
                flee_below: 0.3,
                flee_speed: 200.0,
                hide_ticks: 360,
                respawn_ticks: 2400,
                notice_moving: 0.0,
                scent_ticks: 120, // hunts by smell (real); light makes no difference
                strike: StrikeTuning {
                    damage: 15.0,
                    poise: 40.0,
                    ahead: 4.0,
                    height: 12.0, // the middle of its head
                    radius: 12.0, // its gape, about the head's depth (tuned)
                    knockback: (0.0, 0.0),
                    hitstop_ticks: 10,
                    parryable: true,
                    blockable: true,
                    block_damage: None,
                    armour_break_ticks: 0,
                    follow_up: None,
                    floor_len: 0.0,
                    lunge: 130.0, // 21 cm: about a third of its length (tuned)
                    grab: Some(GrabTuning {
                        ticks: 150,
                        struggle: 8,
                        every: 30,
                        damage: 4.0,
                    }),
                },
            },
            memory: MemoryTuning {
                cell: 32.0, // 5 cm
                step_ticks: 12,
                // The usual view's corners on a wide screen (58 cm): what's
                // on screen at the usual zoom is what the crab perceives
                // (tuned; its lantern lights only 18 cm, beyond that dim
                // shapes and movement).
                sight: 360.0,
                sight_tall: 1.0,
                sight_per_culture: 40.0,
                visit_gap_steps: 300, // away 30 s, back again: one more visit
                // Shore crabs ran a maze without error by the third run (real);
                // a place is learned the same way.
                learned: [0.55, 0.8, 1.0],
                // Two weeks without the maze, they still found the way, more
                // slowly (real): a remembered place keeps some of itself.
                kept: 0.35,
                half_life_steps: 1800, // 3 min per visit (tuned: weeks into minutes)
            },
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
