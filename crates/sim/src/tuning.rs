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
    /// (with 10: 20 for level 2, 60 for 3, 120 for 4, 200 for 5).
    pub climb_level_xp: f32,
    /// Extra climbing speed per Climbing level above 1 (a share of
    /// `climb_speed`).
    pub climb_speed_per_level: f32,

    // --- Skills (`crate::skills`): grow by use ----------------------------
    /// Swimming experience per paddle stroke.
    pub swim_xp_per_stroke: f32,
    /// Swimming levels: level n+1 needs n x (n+1) times this much experience
    /// (with 5: 10 strokes for level 2, 30 for 3, 60 for 4, 100 for 5).
    pub swim_level_xp: f32,
    /// Per Swimming level above 1, the stroke rhythm and the paddle legs'
    /// rest on solid ground are this share shorter.
    pub swim_quicker_per_level: f32,
    /// Fighting experience per point of damage the claws deal a creature.
    pub fight_xp_per_damage: f32,
    /// Fighting experience for beating a creature.
    pub fight_xp_per_win: f32,
    /// Fighting levels: level n+1 needs n x (n+1) times this much experience
    /// (with 20: 40 for level 2, 120 for 3, 240 for 4, 400 for 5).
    pub fight_level_xp: f32,
    /// Extra claw damage per Fighting level above 1 (a share of the blow).
    pub fight_damage_per_level: f32,
    /// Swimming and Fighting stop getting stronger past this level.
    pub skill_cap: u32,

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
    /// How much shell eating a beaten creature mends (it fills the reserve
    /// too): the reason to fight rather than slip past.
    pub meal_mend: f32,
    /// How long the crab takes to back into its home shell, and to come back
    /// out (seconds; see [`crate::home`]). Out is quicker: crabs dart out.
    pub home_in_time: f32,
    pub home_out_time: f32,
    /// The crusher's smash: slow, heavy, committed.
    pub crusher: ClawTuning,
    /// The cutter's jab: quick and light.
    pub cutter: ClawTuning,
    /// The claws strike what they're aimed at: a blow comes down this far
    /// at most onto something short or a step below...
    pub aim_down: f32,
    /// ...or up this far at most at something a little above (units; a
    /// bulked-up crab's reach further).
    pub aim_up: f32,
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
    /// The common octopus, a boss in its den: hidden as part of the rock, it
    /// pounces, then probes with its arm tips (`creatures/common-octopus.md`).
    pub octopus: OctopusTuning,
    /// Scissor claws (the shed tooth fitted to the crusher): a jab cuts soft
    /// parts, like an octopus's arm, this many times as deep.
    pub scissor_cut: f32,
    /// Venom from a bite or sting that gets through (an octopus's).
    pub venom: VenomTuning,
    /// Dropping a walking leg to get free of a hold.
    pub autotomy: AutotomyTuning,
    /// What a crab sees and remembers (`crate::memory`).
    pub memory: MemoryTuning,
    /// Hulk mode, the `hulk` code (`crate::codes`).
    pub hulk: HulkTuning,
    /// How many small things (clams, shell grit) cling to the crab's shell
    /// at once, the way a decorator crab wears bits of the floor. A big find
    /// (a culture, a tooth) rides on its back besides, one at a time.
    pub cling: usize,
    /// Clams, the larder at home and being well fed (`docs/design/world.md`).
    pub food: FoodTuning,
    /// Growing the home shell with shell grit (`docs/design/world.md`).
    pub build: BuildTuning,
    /// The workbench at home (`crate::craft`).
    pub craft: CraftTuning,
    /// The canyon's tide: water running up its middle, then down
    /// (`docs/design/world.md`, "The canyon").
    pub tide: TideTuning,
    /// Feats and moulting (`crate::moult`).
    pub moult: MoultTuning,
}

/// Feats and moulting: when a feat is ready, and the soft time after a
/// moult. Times in ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct MoultTuning {
    /// A skill at this level makes its feat ready.
    pub feat_level: u32,
    /// How long a crab stays soft after a moult, out in the open...
    pub soft_ticks: u32,
    /// ...and tucked in, it hardens this many times as fast.
    pub harden_tucked: u32,
    /// Soft, the shell holds this share of what it would...
    pub soft_shell: f32,
    /// ...and the claws' blows do this share.
    pub soft_claws: f32,
    /// Each moult, hardened, adds this much to what the shell holds.
    pub growth: f32,
    /// Broad paddles: the flip in the water drives the crab this fast the
    /// way it pushes (units a second)...
    pub dash_speed: f32,
    /// ...for this long (seconds).
    pub dash_time: f32,
    /// Heavy crusher: the crusher can be held to charge for this many ticks
    /// even in a feel without a charge (a longer charge, if the feel has
    /// one)...
    pub crusher_charge_ticks: u32,
    /// ...for at least this much more at full charge...
    pub crusher_charge_bonus: f32,
    /// ...and its blows break this many times the poise.
    pub crusher_poise: f32,
}

/// The tide in the canyon below the crack. In a real submarine canyon the
/// tidal current runs along the canyon's axis, up-canyon on the flood and
/// down-canyon on the ebb (Wang, Lee & Liu 2008, sourced in `world.md`).
/// Hours squeezed into a minute: one round is up, slack, down, slack, each
/// change of flow a short ramp. Times in ticks, speeds in units a second.
#[derive(Clone, Debug, PartialEq)]
pub struct TideTuning {
    /// How long the water runs up at full speed...
    pub up_ticks: u32,
    /// ...and down.
    pub down_ticks: u32,
    /// Still water between the two, after each.
    pub slack_ticks: u32,
    /// How long the water takes to speed up or slow down at each change.
    pub ramp_ticks: u32,
    /// The water's speed running up (a crab in it rises at this minus its
    /// sinking speed: about 450 at 800)...
    pub up_speed: f32,
    /// ...and running down (it sinks at this plus its sinking speed).
    pub down_speed: f32,
}

impl TideTuning {
    /// Ticks in one round of the tide.
    pub fn period(&self) -> u32 {
        self.up_ticks + self.down_ticks + 2 * self.slack_ticks + 4 * self.ramp_ticks
    }

    /// How the water runs at tick `tick`: 1 = up at full speed, -1 = down,
    /// 0 = slack. The round starts with the water speeding up to run up.
    pub fn flow(&self, tick: u64) -> f32 {
        let period = u64::from(self.period().max(1));
        let mut t = (tick % period) as f32;
        let ramp = self.ramp_ticks.max(1) as f32;
        // Each stretch: (length, flow at its start, flow at its end).
        let stretches = [
            (ramp, 0.0, 1.0),
            (self.up_ticks as f32, 1.0, 1.0),
            (ramp, 1.0, 0.0),
            (self.slack_ticks as f32, 0.0, 0.0),
            (ramp, 0.0, -1.0),
            (self.down_ticks as f32, -1.0, -1.0),
            (ramp, -1.0, 0.0),
            (self.slack_ticks as f32, 0.0, 0.0),
        ];
        for (length, from, to) in stretches {
            if t < length {
                return from + (to - from) * t / length;
            }
            t -= length;
        }
        0.0
    }

    /// The water's upward speed for a flow (see [`TideTuning::flow`]).
    pub fn speed(&self, flow: f32) -> f32 {
        if flow >= 0.0 {
            flow * self.up_speed
        } else {
            flow * self.down_speed
        }
    }
}

/// Hulk mode: the crab bulks up, hits far harder and is hard to shake.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HulkTuning {
    /// How much bigger it is drawn, and how much its claws' reach and its
    /// body box grow in a fight. (Its box against the level stays the same,
    /// so it still fits through every gap.)
    pub scale: f32,
    /// Its smash's damage and poise damage, times this.
    pub smash_damage: f32,
    /// Its smash's knockback, times this.
    pub smash_knockback: f32,
    /// Share of a blow's poise damage it feels.
    pub poise_taken: f32,
    /// Share of its walking (and sprinting) speed.
    pub walk: f32,
}

/// Food: clams carried home to the larder, eaten resting at home, and the
/// well-fed time that follows (Valheim's food). Times in ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct FoodTuning {
    /// How many clams the larder at home holds.
    pub larder: u32,
    /// A taken clam bed grows a new clam after this long.
    pub regrow_ticks: u32,
    /// Tucked in at home this long, a crab that isn't well fed eats one.
    pub eat_after_ticks: u32,
    /// How long a meal keeps it well fed. It ends all at once.
    pub well_fed_ticks: u32,
    /// Well fed, the shell holds this share more (filled when it eats)...
    pub shell_bonus: f32,
    /// ...and the reserve this many strokes more.
    pub reserve_bonus: u32,
}

/// Building: shell grit carried home and built into the home shell, which
/// grows in stages, each felt at home. Times in ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildTuning {
    /// How much grit built in, in all, makes each stage (stage 1 at the
    /// first, and so on).
    pub stages: [u32; 3],
    /// Stage 1, the doorway shored up: tucked in at home, the shell mends
    /// after this long unhit (instead of `home_heal_after_ticks`).
    pub mend_after_ticks: u32,
    /// Stage 2, a shelf of grit inside: the larder holds this many clams.
    pub larder: u32,
    /// Stage 3, the lip grown out into a deeper home: a meal eaten there
    /// keeps the crab well fed this many times as long.
    pub meal_longer: f32,
}

/// The workbench at home: what eating leaves for it, and what its gear
/// does (`crate::craft`; what each recipe needs is in its table).
#[derive(Clone, Debug, PartialEq)]
pub struct CraftTuning {
    /// Empty valves a clam eaten at home leaves at the workbench (a clam
    /// has two).
    pub valves_per_clam: u32,
    /// With clam-shell guards on the claws, a blocked blow lets this share
    /// of what a block lets through reach the shell.
    pub guard_share: f32,
}

impl BuildTuning {
    /// The stage a home with `grit` built into it has reached (0: none).
    pub fn stage(&self, grit: u32) -> u32 {
        self.stages.iter().filter(|&&n| grit >= n).count() as u32
    }
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
    /// A stalker: through its tell it closes in on a crab out of reach,
    /// and while it rears (before the cocked hold) it turns after one that
    /// slipped past. Others hold still: backing off makes their blow miss.
    pub stalks: bool,
    /// Hunts by sight: a crab in an ink cloud (blinded) is lost to it.
    pub sight: bool,
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

/// The common octopus (`crate::octopus`). Its pounce, senses and health are
/// an ordinary [`EnemyTuning`]; its eyes and arms are its own.
#[derive(Clone, Debug, PartialEq)]
pub struct OctopusTuning {
    /// Health and poise (the mantle), the mantle and head (`body`: an
    /// ellipse this wide and tall, standing on the floor), sight
    /// (`notice_range`), and the pounce: its tell (the dark flush), its
    /// active ticks, its range (`strike_range`, from its front) and blow.
    pub base: EnemyTuning,
    /// Each eye's hurtbox, a circle this big...
    pub eye_radius: f32,
    /// ...this far ahead of its middle and this high, flattened on the rock
    /// while hidden...
    pub eye_hidden: (f32, f32),
    /// ...and once it has risen.
    pub eye_up: (f32, f32),
    /// A blow to an eye counts this many times.
    pub eye_damage: f32,
    pub arms: ArmTuning,
    pub webover: WeboverTuning,
    /// Its bites while it holds a crab.
    pub bite: BiteTuning,
    pub ink: InkTuning,
    pub den: DenTuning,
}

/// Hurt, it inks and jets away to fight on from elsewhere.
#[derive(Clone, Debug, PartialEq)]
pub struct InkTuning {
    /// At these shares of its health, once each...
    pub at: [f32; 2],
    /// ...it inks and jets for this long (unhittable), then reappears...
    pub jet_ticks: u32,
    /// ...at least this far away, on floor no more than `climb` above or
    /// below its own.
    pub jet: f32,
    pub climb: f32,
    /// The cloud: this big round where it was, for this long. A crab inside
    /// is blinded.
    pub radius: f32,
    pub cloud_ticks: u32,
}

/// What its den holds once it has fled for good.
#[derive(Clone, Debug, PartialEq)]
pub struct DenTuning {
    /// Pieces of shell grit in its midden (per crab).
    pub midden: u32,
}

/// The webover: it rises over a crab close by, its web spread like an
/// umbrella, and drops it like a net.
#[derive(Clone, Debug, PartialEq)]
pub struct WeboverTuning {
    /// It rises when the crab's middle is this close to its own...
    pub range: f32,
    /// ...and there is this much clear water over the crab (not in a low
    /// tunnel).
    pub headroom: f32,
    /// The tell: rising, the web spreading. Its aim follows the crab until
    /// the last `lock_ticks` of it.
    pub rise_ticks: u32,
    pub lock_ticks: u32,
    /// The drop: the web comes down from `height` to the floor over this
    /// long, this wide, catching what's under it.
    pub drop_ticks: u32,
    pub width: f32,
    pub height: f32,
    /// What the web does to a crab it catches (then it holds on).
    pub damage: f32,
    pub poise: f32,
    /// Missed: it lies with its arms spread on the floor this long, and a
    /// blow to an arm then counts this many times.
    pub recover_ticks: u32,
    pub arm_damage: f32,
    /// After a webover, this long before the next.
    pub rest_ticks: u32,
}

/// An octopus's bites into the crab it holds, by how long it has held it.
#[derive(Clone, Debug, PartialEq)]
pub struct BiteTuning {
    /// The venom bite...
    pub venom_at: u32,
    pub venom_damage: f32,
    /// ...and the heavy one.
    pub heavy_at: u32,
    pub heavy_damage: f32,
}

/// Venom in the crab: each dose for `ticks`, `per_second` damage a dose,
/// and it walks at `slow` of its speed while any is in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VenomTuning {
    pub ticks: u32,
    pub per_second: f32,
    pub slow: f32,
}

/// Autotomy: a crab held fast drops a walking leg to get free at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutotomyTuning {
    /// Holding dodge this long while held drops one too (on a phone, with no
    /// down to press).
    pub hold_ticks: u32,
    /// Each leg gone takes this share off its walking...
    pub walk_per_leg: f32,
    /// ...and it can drop at most this many.
    pub most: u32,
}

/// An octopus's arms: each its own health, probing for the crab.
#[derive(Clone, Debug, PartialEq)]
pub struct ArmTuning {
    pub count: usize,
    /// Each arm's own health: at 0 it is pulled back for the rest of the
    /// fight.
    pub health: f32,
    /// How far an arm reaches from its root.
    pub length: f32,
    /// Its hurtbox: a chain of this many circles, root to tip...
    pub segments: usize,
    /// ...tapering from the first's radius to the last's.
    pub radius: (f32, f32),
    /// The roots, in a ring under the head: this far ahead of its middle
    /// and this high.
    pub root: (f32, f32),
    /// A tip feels along this high above the floor...
    pub height: f32,
    /// ...and grabs whatever this circle round it touches.
    pub tip_radius: f32,
    /// The tell: a tip slides in for this long, to where it touches the
    /// crab...
    pub tell_ticks: u32,
    /// ...then feels about there this long before drawing back.
    pub feel_ticks: u32,
    /// Drawing an arm back in.
    pub back_ticks: u32,
    /// A parried tip snaps back in this long, and stays curled, sore, this
    /// long (counted from the parry).
    pub snap_ticks: u32,
    pub recoil_ticks: u32,
    /// Arms out probing at once...
    pub at_once: usize,
    /// ...and the least time between two starting.
    pub gap_ticks: u32,
    /// A tip's touch: its damage, poise damage and hit-stop. It grabs with
    /// the pounce's hold.
    pub damage: f32,
    pub poise: f32,
    pub hitstop_ticks: u32,
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
            climb_level_xp: 10.0,    // level 2 at 20, 3 at 60, 4 at 120, 5 at 200
            climb_speed_per_level: 0.08,

            swim_xp_per_stroke: 1.0,
            swim_level_xp: 5.0, // level 5 at 100 strokes: about the middle of a demo run
            swim_quicker_per_level: 0.05,
            fight_xp_per_damage: 0.25,
            fight_xp_per_win: 15.0, // a snapper is worth 18, the mantis 30
            fight_level_xp: 20.0,   // level 2 after beating two, 5 at 400
            fight_damage_per_level: 0.05,
            skill_cap: 10,

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
            meal_mend: 50.0, // half a shell
            home_in_time: 0.45,
            home_out_time: 0.25,
            // Hit-stop 7: about 60 ms, you feel it land. Reach: the claw's
            // length past the shell (the longer chelipeds, #90).
            crusher: ClawTuning::plain((14, 6, 22), 12.0, 25.0, 44.0, 30.0, (320.0, 0.0), 7),
            // A touch shorter than the crusher (tuned).
            cutter: ClawTuning::plain((6, 4, 12), 5.0, 8.0, 38.0, 24.0, (90.0, 0.0), 3),
            // Down to a little below the feet (a creature in a dip); up to
            // a little past the top of the shell (one on a ledge, or
            // swimming low). Further misses.
            aim_down: 16.0,
            aim_up: 28.0,
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
                stalks: false,
                sight: true,
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
                give_up_range: 800.0,
                approach_speed: 330.0, // darts: faster than the crab walks, not than it sprints
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
                stalks: true, // stalks prey and darts after it (not verified)
                sight: true,  // a sight hunter (its eyes: `mantis-shrimp.md`)
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
                stalks: false,
                sight: false, // feels steps
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
                stalks: false,
                sight: false, // hunts by smell
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
            // "Page" = the spec table on `creatures/common-octopus.md`; "real"
            // = from its sources; everything else is tuned.
            octopus: OctopusTuning {
                base: EnemyTuning {
                    vitals: VitalsTuning {
                        health: 400.0, // page
                        poise: 200.0,  // page
                        poise_refill: 30.0,
                        poise_delay_ticks: 90,
                        stagger_ticks: 70,
                    },
                    // Mantle 25 cm (real); the mantle and head as the page's
                    // ellipse, 156 x 110.
                    body: (156.0, 110.0),
                    head: 0.0,
                    head_recover: 1.0,
                    tail: 0.0,
                    tail_damage: 1.0,
                    burrows: false, // hidden in plain sight instead (`crate::octopus`)
                    notice_range: 400.0, // by sight (page)
                    notice_ticks: 0,
                    give_up_range: 900.0,
                    approach_speed: 0.0, // keeps to its den
                    // Pounces on a crab passing this close to its front (page).
                    strike_range: 140.0,
                    pause_ticks: (0, 0),
                    tell_ticks: 30, // page: only a dark flush of the skin
                    cocked_ticks: 0,
                    recock_ticks: 0,
                    combo_extra: 0,
                    active_ticks: 10, // page
                    recover_ticks: 60,
                    parried_ticks: 60,
                    flee_below: 0.0,
                    flee_speed: 0.0,
                    hide_ticks: 0,
                    respawn_ticks: 0, // a boss: no other moves in
                    notice_moving: 0.0,
                    scent_ticks: 0,
                    stalks: false,
                    sight: true, // hunts by sight (page); in ink, by touch
                    strike: StrikeTuning {
                        damage: 20.0, // page
                        poise: 40.0,
                        ahead: 0.0,
                        height: 30.0,
                        radius: 40.0, // its body landing on the crab
                        knockback: (0.0, 0.0),
                        hitstop_ticks: 10,
                        parryable: true,
                        blockable: true,
                        block_damage: None,
                        armour_break_ticks: 0,
                        follow_up: None,
                        floor_len: 0.0,
                        lunge: 140.0, // the whole pounce range
                        // Held up to 240, each dodge press 10 off (page).
                        grab: Some(GrabTuning {
                            ticks: 240,
                            struggle: 10,
                            every: 0, // no bites yet
                            damage: 0.0,
                        }),
                    },
                },
                eye_radius: 12.0, // page
                eye_hidden: (44.0, 40.0),
                eye_up: (40.0, 92.0),
                eye_damage: 2.0,
                arms: ArmTuning {
                    count: 8,
                    health: 40.0,        // page
                    length: 625.0,       // 1 m (real)
                    segments: 6,         // page
                    radius: (14.0, 3.0), // page
                    root: (55.0, 14.0),
                    height: 12.0,
                    tip_radius: 14.0, // page
                    tell_ticks: 60,   // page
                    feel_ticks: 120,
                    back_ticks: 40,
                    snap_ticks: 10,
                    recoil_ticks: 150,
                    at_once: 2, // page
                    gap_ticks: 45,
                    damage: 5.0,
                    poise: 15.0,
                    hitstop_ticks: 6,
                },
                webover: WeboverTuning {
                    range: 250.0, // half the web
                    headroom: 220.0,
                    rise_ticks: 84, // page (real timing not measured)
                    lock_ticks: 24,
                    drop_ticks: 10, // page
                    width: 500.0,   // page
                    height: 150.0,
                    damage: 8.0,
                    poise: 30.0,
                    recover_ticks: 70, // page
                    arm_damage: 1.5,   // page
                    rest_ticks: 120,
                },
                bite: BiteTuning {
                    venom_at: 120, // page
                    venom_damage: 10.0,
                    heavy_at: 200, // page
                    heavy_damage: 30.0,
                },
                ink: InkTuning {
                    at: [0.66, 0.33], // page
                    jet_ticks: 30,    // page
                    jet: 600.0,       // page: 600+
                    climb: 200.0,
                    radius: 160.0,    // page; about the animal's size (real: Wikipedia)
                    cloud_ticks: 180, // page
                },
                den: DenTuning { midden: 4 },
            },
            scissor_cut: 2.0, // `creatures/README.md`: x2 vs soft parts and arms
            // The octopus's page: slow 50%, 2 damage/s for 5 s, stacks.
            venom: VenomTuning {
                ticks: 600,
                per_second: 2.0,
                slow: 0.5,
            },
            // `creatures/README.md`: -8% walking per missing leg.
            autotomy: AutotomyTuning {
                hold_ticks: 60,
                walk_per_leg: 0.08,
                most: 4,
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
            hulk: HulkTuning {
                scale: 1.5,
                smash_damage: 3.0,
                smash_knockback: 2.5,
                poise_taken: 0.2,
                walk: 0.8,
            },
            // Four: a shallows trip (two clams, a heap of grit) fits with
            // room to spare, and so does the pit (a clam, two heaps), but the
            // larder (5) can't be filled nor the home (5 grit) finished in
            // one trip.
            cling: 4,
            food: FoodTuning {
                larder: 5,
                regrow_ticks: 3 * 60 * 120,   // 3 min of play
                eat_after_ticks: 60,          // half a second settled in
                well_fed_ticks: 4 * 60 * 120, // 4 min of play: about one dive
                shell_bonus: 0.25,
                reserve_bonus: 3,
            },
            build: BuildTuning {
                stages: [1, 3, 5],
                mend_after_ticks: 60, // half a second, from 2 s
                larder: 8,
                meal_longer: 1.5, // 6 min of play, from 4
            },
            // One clam's two valves make the guards. A block then lets 10% of
            // a blow through instead of 20% (20% instead of 40% with the guard
            // cracked): noticed over a fight, never a free pass.
            craft: CraftTuning {
                valves_per_clam: 2,
                guard_share: 0.5,
            },
            tide: TideTuning {
                up_ticks: 24 * 120,   // long enough to ride the whole canyon up
                down_ticks: 16 * 120, // a quicker, harder ebb
                slack_ticks: 6 * 120,
                ramp_ticks: 2 * 120, // one round: a minute
                up_speed: 800.0,
                down_speed: 500.0,
            },
            moult: MoultTuning {
                feat_level: 5,
                soft_ticks: 90 * 120, // 90 s of play out in the open
                harden_tucked: 3,     // 30 s resting tucked in
                soft_shell: 0.5,
                soft_claws: 0.6,
                growth: 10.0,             // a tenth of the starting shell
                dash_speed: 800.0,        // a little faster than the dodge on the floor
                dash_time: 0.25,          // about 200 further each flip
                crusher_charge_ticks: 60, // half a second
                crusher_charge_bonus: 0.5,
                crusher_poise: 1.5,
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
