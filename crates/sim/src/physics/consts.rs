/// The physics runs in fixed steps of this many seconds. A fixed step makes
/// the motion the same on a 60 Hz phone and a 144 Hz monitor (and, later,
/// on every player's computer).
pub const STEP: f32 = 1.0 / TICKS_PER_SECOND as f32;

/// How many physics steps ("ticks") make one second of game time.
pub const TICKS_PER_SECOND: u32 = 120;
