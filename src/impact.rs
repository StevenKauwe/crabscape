//! How a blow lands, as the player sees and hears it: the camera shake and
//! kick, the zoom punch on a counter, the sounds, and the debris. Scaled by
//! the blow (which claw, momentum or a charge, a counter) and by the feel
//! preset's [`Juice`](crate::feel::Juice). It only reads the simulation.
//!
//! A whiff and a hit are told apart on purpose: every blow swishes through
//! the water, but only a hit crunches, freezes (in the sim), shakes and
//! throws chips.

use bevy::prelude::*;
use crabsim::combat::{Claw, Phase};
use crabsim::{Fighter, Mover, Sim};

use crate::effects::{Burst, BurstKind};
use crate::feel::FeelLab;
use crate::player::{FrameEvents, GameSet, LocalPlayer};
use crate::sound::{PlaySfx, Sfx};
use crate::view::Shake;

pub struct ImpactPlugin;

impl Plugin for ImpactPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, react.in_set(GameSet::Present));
    }
}

/// How much of a smash a jab is worth, for shake, debris and loudness.
const JAB_WEIGHT: f32 = 0.35;

#[allow(clippy::too_many_arguments)]
fn react(
    lab: Res<FeelLab>,
    sim: Res<Sim>,
    crab: Query<(&Mover, &Fighter, &FrameEvents), With<LocalPlayer>>,
    mut shake: ResMut<Shake>,
    mut sfx: MessageWriter<PlaySfx>,
    mut bursts: MessageWriter<Burst>,
    // The swing (its claw and wind-up length) whose blow is already out.
    mut out: Local<Option<(Claw, u32)>>,
) {
    let Ok((m, f, ev)) = crab.single() else {
        return;
    };
    let j = lab.juice();
    let t = &sim.tuning;
    let ev = ev.0;
    let mut play = |s: Sfx, gain: f32, pitch: f32| {
        sfx.write(PlaySfx {
            sfx: s,
            gain: gain * j.sound,
            pitch: pitch * j.pitch,
        });
    };

    let swing = f.swing.as_ref();
    if ev.smashed || ev.jabbed {
        *out = None; // a new swing started this frame
    }
    let active = swing.filter(|s| s.phase(t) == Phase::Active);
    // A blow comes out: it swishes through the water, hit or not.
    let this = active.map(|s| (s.claw, s.windup_len()));
    if this.is_some() && this != *out {
        let s = active.expect("checked");
        let heavy = s.claw == Claw::Crusher;
        play(
            Sfx::Swish,
            if heavy { 0.55 } else { 0.3 },
            if heavy { 0.75 } else { 1.3 },
        );
        // The feet dig in and push off: sand behind them.
        if ev.lunged && j.debris > 0.0 {
            bursts.write(Burst {
                kind: BurstKind::Sand,
                at: m.pos - Vec2::X * f.facing * 20.0,
                dir: -f.facing,
                count: (j.debris * 0.4 * if heavy { 1.0 } else { 0.5 }) as u32,
                strength: 0.7,
            });
        }
    }
    *out = this.or(if swing.is_some_and(|s| s.phase(t) == Phase::Recovery) {
        *out
    } else {
        None
    });

    // The crush: clamp on, squeeze crack after crack (each louder, higher,
    // shakier), then let go with the counter's full blow.
    let at = f
        .hitbox(m, t)
        .map_or(m.pos + Vec2::new(f.facing * 70.0, 25.0), |b| {
            b.center() + Vec2::X * f.facing * b.width() * 0.4
        });
    if ev.seized {
        play(Sfx::Thud, 1.0, 0.7);
        play(Sfx::Click, 0.9, 0.6);
        shake.add(j.shake * 0.5);
        shake.kick(Vec2::new(f.facing, -0.3) * j.kick * 0.5);
    }
    if ev.cracked {
        let n = f.crush.map_or(1, |c| c.cracks) as f32;
        play(Sfx::Crunch, 0.5 + 0.2 * n, 1.25 + 0.15 * n);
        shake.add(j.shake * (0.25 + 0.15 * n));
        bursts.write(Burst {
            kind: BurstKind::Shards,
            at,
            dir: f.facing,
            count: (j.debris * (0.25 + 0.15 * n)) as u32,
            strength: 0.6 + 0.2 * n,
        });
    }
    if ev.crushed {
        let power = j.counter;
        play(Sfx::Boom, 1.2, 0.9);
        play(Sfx::Crunch, 1.2, 0.8);
        shake.add(j.shake * power);
        shake.kick(Vec2::new(f.facing, -0.35) * j.kick * power.sqrt());
        shake.punch(j.zoom);
        bursts.write(Burst {
            kind: BurstKind::Shards,
            at,
            dir: f.facing,
            count: (j.debris * power) as u32,
            strength: power.min(2.5),
        });
        bursts.write(Burst {
            kind: BurstKind::Sand,
            at: Vec2::new(at.x, m.pos.y),
            dir: f.facing,
            count: (j.debris * power * 0.5) as u32,
            strength: power.min(2.5),
        });
    }
    if ev.respawned && j.debris > 0.0 {
        for dir in [-1.0, 1.0] {
            bursts.write(Burst {
                kind: BurstKind::Sand,
                at: m.pos,
                dir,
                count: 8,
                strength: 0.8,
            });
        }
    }

    if ev.struck
        && !ev.seized
        && let Some(s) = swing
    {
        let heavy = s.claw == Claw::Crusher;
        let weight = if heavy { 1.0 } else { JAB_WEIGHT };
        let payoff = if s.counter { j.counter } else { 1.0 };
        let power = s.strength(t) * payoff;
        shake.add(j.shake * weight * power);
        shake.kick(Vec2::new(f.facing, -0.35) * j.kick * weight * power.sqrt());
        if s.counter {
            shake.punch(j.zoom);
            play(Sfx::Boom, 1.0, 1.0);
        }
        // Harder blows sound lower and louder.
        let deeper = 1.0 / power.sqrt().max(0.6);
        if heavy {
            play(Sfx::Crunch, (0.8 * power).min(1.2), deeper);
        } else {
            play(Sfx::Click, 0.7, deeper);
        }
        if let Some(b) = f.hitbox(m, t) {
            let at = b.center() + Vec2::X * f.facing * b.width() * 0.4;
            let count = (j.debris * weight * power).round() as u32;
            bursts.write(Burst {
                kind: BurstKind::Shards,
                at,
                dir: f.facing,
                count,
                strength: power.min(2.5),
            });
            bursts.write(Burst {
                kind: BurstKind::Sand,
                at: Vec2::new(at.x, m.pos.y),
                dir: f.facing,
                count: count / 2,
                strength: power.min(2.5),
            });
        }
    }
    if ev.parried {
        play(Sfx::Ring, 0.9, 1.0);
        shake.add(j.shake * 0.4);
        shake.punch(j.zoom * 0.5);
        bursts.write(Burst {
            kind: BurstKind::Shards,
            at: m.pos + Vec2::new(f.facing * 44.0, 34.0),
            dir: f.facing,
            count: (j.debris * 0.5) as u32,
            strength: 0.8,
        });
    }
    if ev.hurt {
        // A club landing on the crab: the snap, and the crab's own shell.
        play(Sfx::Snap, 0.9, 1.0);
        play(Sfx::Thud, 0.8, 1.0);
        shake.add(j.shake * 0.9);
        shake.kick(Vec2::new(-f.facing, -0.5) * j.kick * 0.8);
    } else if ev.blocked {
        play(Sfx::Snap, 0.6, 1.1);
        play(Sfx::Thud, 0.5, 1.3);
        shake.add(j.shake * 0.3);
    }
}
