//! Legacy `updateShots` and `hitEnemy`, with explicit world authority and stable IDs.
//! Presentation (hit-stop, squash, particles, sound and text), rewards, split spawns,
//! boss add calming and networking remain caller callbacks, represented by events.
//! No presentation random draws are consumed: this core does not share their RNG.
use crate::combat::{Kind, Shot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub hp: f64,
    pub max_hp: f64,
    pub boss: bool,
    pub armor: f64,
    pub marked: f64,
    pub cracked: f64,
    pub slow: f64,
    pub stun: f64,
    pub flash: f64,
    pub dead: bool,
    pub calm: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Authority {
    pub guest: bool,
    /// Damage accumulated by siblings, as returned by legacy `sibBossDmg()`.
    pub sibling_boss_damage: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct ShotWorld {
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub player: [f64; 2],
    pub authority: Authority,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Defeat {
        id: u64,
        owner: String,
    },
    DadImpact {
        x: f64,
        y: f64,
    },
    GuestHit {
        id: u64,
        dmg: f64,
        kind: Kind,
        slow: f64,
        stun: f64,
        mega: bool,
        mark: bool,
        crack: bool,
    },
}

/// Apply one hit. Mark/crack bonuses use the state *before* this hit applies them.
/// Guest mirrors flash and slow immediately, but do not change any health or stun.
pub fn hit_enemy(e: &mut Target, s: &Shot, authority: Authority, events: &mut Vec<Event>) {
    if e.dead || e.calm {
        return;
    }
    if authority.guest {
        e.flash = 0.12;
        if s.slow != 0.0 {
            e.slow = s.slow;
        }
        events.push(Event::GuestHit {
            id: e.id,
            dmg: s.dmg,
            kind: s.kind,
            slow: s.slow,
            stun: s.stun,
            mega: s.mega,
            mark: s.mark,
            crack: s.crack,
        });
        return;
    }
    let mut dmg = if s.mega {
        if e.boss { e.max_hp * 0.4 } else { e.hp + 1.0 }
    } else {
        s.dmg
    };
    if e.marked > 0.0 && !s.mega {
        dmg *= 1.4;
    }
    if e.armor != 0.0 && !s.mega && e.cracked <= 0.0 {
        dmg *= match s.kind {
            Kind::Boulder => 1.6,
            Kind::Arrow | Kind::Hammer | Kind::Area => 1.0,
            _ => e.armor,
        };
    }
    if e.cracked > 0.0 && e.boss && !s.mega {
        dmg *= 1.5;
    }
    if (s.kind == Kind::Hammer || s.crack) && (e.armor != 0.0 || e.boss) {
        e.cracked = 6.0;
    }
    if s.mark {
        e.marked = if e.boss { 3.0 } else { 6.0 };
    }
    e.hp -= dmg;
    e.flash = 0.12;
    if s.slow != 0.0 {
        e.slow = s.slow;
    }
    if s.stun != 0.0 {
        e.stun = s.stun;
    }
    if (if e.boss {
        e.hp - authority.sibling_boss_damage
    } else {
        e.hp
    }) <= 0.0
    {
        e.dead = true;
        events.push(Event::Defeat {
            id: e.id,
            owner: s.owner.clone(),
        });
    }
}

/// Uses the caller's original frame duration, with no hidden clamp or subdivision.
/// Targets must have unique stable IDs. Both input orders are gameplay-significant.
/// Events are emitted in collision order; apply world/reward callbacks separately.
pub fn update_shots(
    shots: &mut Vec<Shot>,
    enemies: &mut [Target],
    dt: f64,
    world: ShotWorld,
) -> Vec<Event> {
    let mut events = Vec::new();
    for i in (0..shots.len()).rev() {
        let s = &mut shots[i];
        s.life -= dt;
        if s.kind == Kind::Hammer {
            s.age += dt;
            s.spin += dt * 14.0;
            if s.age > 0.45 {
                let dx = world.player[0] - s.x;
                let dy = world.player[1] - s.y;
                let distance = dx.hypot(dy);
                let d = if distance == 0.0 { 1.0 } else { distance };
                let k = (dt * 5.0).min(1.0);
                s.vx += (dx / d * world.scale * 11.0 - s.vx) * k;
                s.vy += (dy / d * world.scale * 11.0 - s.vy) * k;
                if d < world.scale * 0.5 {
                    shots.remove(i);
                    continue;
                }
            }
        }
        if s.homing {
            let mut nearest = None;
            let mut best_distance = f64::INFINITY;
            for e in enemies.iter() {
                if e.calm || e.dead {
                    continue;
                }
                let distance = (e.x - s.x).hypot(e.y - s.y);
                if distance < best_distance {
                    best_distance = distance;
                    nearest = Some(e);
                }
            }
            if let Some(e) = nearest {
                let speed = s.vx.hypot(s.vy);
                let angle = s.vy.atan2(s.vx);
                let desired = (e.y - s.y).atan2(e.x - s.x);
                let turn = ((desired - angle + std::f64::consts::PI * 3.0) % std::f64::consts::TAU
                    - std::f64::consts::PI)
                    .clamp(-6.0 * dt, 6.0 * dt);
                s.vx = (angle + turn).cos() * speed;
                s.vy = (angle + turn).sin() * speed;
            }
        }
        if s.kind == Kind::Boulder {
            s.spin += dt * 5.0;
        }
        s.x += s.vx * dt;
        s.y += s.vy * dt;
        let mut dead = false;
        for e in enemies.iter_mut() {
            if e.dead || e.calm || (s.x - e.x).hypot(s.y - e.y) >= e.r * 0.9 + s.r {
                continue;
            }
            if s.pierce {
                if !s.hits.insert(e.id) {
                    continue;
                }
                hit_enemy(e, s, world.authority, &mut events);
                if s.kind == Kind::Arrow && s.hits.len() >= 2 {
                    dead = true;
                    break;
                }
            } else {
                hit_enemy(e, s, world.authority, &mut events);
                if s.dad {
                    events.push(Event::DadImpact { x: s.x, y: s.y });
                }
                dead = true;
                break;
            }
        }
        if dead
            || s.life <= 0.0
            || s.x < -80.0
            || s.x > world.width + 80.0
            || s.y < -80.0
            || s.y > world.height + 80.0
        {
            shots.remove(i);
        }
    }
    events
}
