//! Player bump/bonk and hazard/strike motion. Retirement world changes are explicit
//! callbacks so callers can apply gem loss and host handoff before later collisions.
use crate::flight::{Player, Viewport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub fn is_safe(p: &Player) -> bool {
    p.dizzy > 0.0
        || p.safe > 0.0
        || p.shield > 0.0
        || p.sila > 0.0
        || p.shadow > 0.0
        || p.down > 0.0
        || p.out
}
pub fn bump(p: &mut Player, source: [f64; 2], scale: f64) {
    p.dizzy = 0.45;
    p.safe = 1.1;
    p.spin = 0.0;
    let dx = p.x - source[0];
    let dy = p.y - source[1];
    let distance = dx.hypot(dy);
    let d = if distance == 0.0 { 1.0 } else { distance };
    p.vx = dx / d * scale * 5.0;
    p.vy = dy / d * scale * 5.0;
}
/// Returns whether the dragon has no hearts; caller decides retirement from mission state.
pub fn bonk(p: &mut Player, source: [f64; 2], scale: f64, armor: f64) -> bool {
    p.dizzy = 1.1 * (1.0 - 0.2 * armor);
    p.safe = 2.6;
    p.spin = 0.0;
    let dx = p.x - source[0];
    let dy = p.y - source[1];
    let distance = dx.hypot(dy);
    let d = if distance == 0.0 { 1.0 } else { distance };
    p.vx = dx / d * scale * 6.0 * (1.0 - 0.2 * armor);
    p.vy = dy / d * scale * 6.0 * (1.0 - 0.2 * armor);
    p.hearts = p.hearts.saturating_sub(1);
    p.hearts == 0
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Hazard {
    pub uid: u64,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub r: f64,
    pub life: f64,
    pub warn: f64,
    pub gravity: f64,
    pub spin: f64,
    pub kind: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Strike {
    pub uid: u64,
    pub x: f64,
    pub y0: Option<f64>,
    pub warn: f64,
    pub bolt: f64,
    pub hit: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum DamageEvent {
    Bonk { source: [f64; 2], kind: String },
    Shield { source: [f64; 2] },
    Zap,
}
#[derive(Default)]
pub struct Used {
    pub hazards: BTreeSet<u64>,
    pub strikes: BTreeSet<u64>,
}

/// Callback runs synchronously after each bonk so retirement/safety changes affect the
/// following hazards. VFX and audio are events; no random draws are consumed here.
#[allow(clippy::too_many_arguments)]
pub fn update_hazards(
    hazards: &mut Vec<Hazard>,
    strikes: &mut Vec<Strike>,
    player: &mut Player,
    dt: f64,
    view: Viewport,
    armor: f64,
    used: &mut Used,
    mut on_bonk: impl FnMut(&mut Player),
) -> Vec<DamageEvent> {
    let mut events = Vec::new();
    let reach = view.scale * view.dragon_scale * 0.35;
    for i in (0..hazards.len()).rev() {
        let h = &mut hazards[i];
        h.life -= dt;
        if h.warn > 0.0 {
            h.warn -= dt;
            continue;
        }
        if h.gravity != 0.0 {
            h.vy += h.gravity * dt;
        }
        h.x += h.vx * dt;
        h.y += h.vy * dt;
        h.spin += dt * 6.0;
        let mut gone = h.life <= 0.0
            || h.x < -60.0
            || h.x > view.width + 60.0
            || h.y > view.ground_y + view.scale * 0.2
            || h.y < -view.scale * 2.0;
        if !gone && (h.x - player.x).hypot(h.y - player.y) < h.r + reach {
            if player.shield > 0.0 {
                events.push(DamageEvent::Shield { source: [h.x, h.y] });
                gone = true;
            } else if !is_safe(player) {
                bonk(player, [h.x, h.y], view.scale, armor);
                events.push(DamageEvent::Bonk {
                    source: [h.x, h.y],
                    kind: h.kind.clone(),
                });
                on_bonk(player);
                gone = true;
            }
            if gone && h.uid != 0 {
                used.hazards.insert(h.uid);
            }
        }
        if gone {
            hazards.remove(i);
        }
    }
    for i in (0..strikes.len()).rev() {
        let s = &mut strikes[i];
        if s.warn > 0.0 {
            s.warn -= dt;
            if s.warn <= 0.0 {
                events.push(DamageEvent::Zap);
            }
            continue;
        }
        s.bolt -= dt;
        if !s.hit
            && (player.x - s.x).abs() < view.scale * 0.6
            && s.y0.is_none_or(|y| player.y > y - view.scale * 0.3)
        {
            s.hit = true;
            if s.uid != 0 {
                used.strikes.insert(s.uid);
            }
            if player.shield <= 0.0 && !is_safe(player) {
                let source = [s.x, player.y - view.scale];
                bonk(player, source, view.scale, armor);
                events.push(DamageEvent::Bonk {
                    source,
                    kind: if s.y0.is_some() {
                        "cloud zap"
                    } else {
                        "lightning"
                    }
                    .into(),
                });
                on_bonk(player);
            }
        }
        if s.bolt <= 0.0 {
            strikes.remove(i);
        }
    }
    events
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warning_consumes_whole_frame_then_shield_records_consumed_id() {
        let mut p = Player {
            x: 100.0,
            y: 100.0,
            shield: 1.0,
            ..Player::default()
        };
        let mut hazards = vec![Hazard {
            uid: 7,
            x: 100.0,
            y: 100.0,
            vx: 10.0,
            r: 5.0,
            life: 1.0,
            warn: 0.01,
            ..Hazard::default()
        }];
        let mut used = Used::default();
        let mut strikes = Vec::new();
        let view = Viewport::from_screen(1000.0, 700.0, 1.0, 40.0, 0.62);
        assert!(
            update_hazards(
                &mut hazards,
                &mut strikes,
                &mut p,
                0.02,
                view,
                0.0,
                &mut used,
                |_| {}
            )
            .is_empty()
        );
        assert_eq!(hazards[0].x, 100.0);
        assert_eq!(
            update_hazards(
                &mut hazards,
                &mut strikes,
                &mut p,
                0.02,
                view,
                0.0,
                &mut used,
                |_| {}
            )
            .len(),
            1
        );
        assert!(hazards.is_empty());
        assert!(used.hazards.contains(&7));
        assert_eq!(p.hearts, 5);
    }
    #[test]
    fn expired_bolts_still_hit_and_retirement_callback_is_synchronous() {
        let mut p = Player {
            x: 100.0,
            y: 100.0,
            hearts: 1,
            ..Player::default()
        };
        let mut strikes = vec![Strike {
            uid: 1,
            x: 100.0,
            bolt: 0.001,
            ..Strike::default()
        }];
        let events = update_hazards(
            &mut vec![],
            &mut strikes,
            &mut p,
            0.02,
            Viewport::from_screen(1000.0, 700.0, 1.0, 40.0, 0.62),
            0.0,
            &mut Used::default(),
            |p| {
                p.out = true;
                p.dizzy = 0.0;
            },
        );
        assert_eq!(events.len(), 1);
        assert!(p.out);
        assert_eq!(p.hearts, 0);
        assert!(strikes.is_empty());
    }
}
