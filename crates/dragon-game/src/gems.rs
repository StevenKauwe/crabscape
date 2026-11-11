//! Gem movement and collection geometry, preserving reverse-array and expiry order.
//! Collection rewards/progression are synchronous callbacks owned by the world.
use crate::flight::{Player, Viewport};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Gem {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub t: f64,
    pub life: f64,
    pub golden: bool,
    pub value: f64,
    pub r: f64,
    pub lost_one: bool,
}
pub fn update_gems(
    gems: &mut Vec<Gem>,
    dt: f64,
    player: &Player,
    view: Viewport,
    sweeping: bool,
    followers: &[[f64; 2]],
    mut collect: impl FnMut(Gem),
) {
    let reach = view.scale * (0.42 + 0.2 * view.dragon_scale);
    for i in (0..gems.len()).rev() {
        let g = &mut gems[i];
        g.life -= dt;
        g.t += dt;
        if player.magnet > 0.0 || sweeping {
            let dx = player.x - g.x;
            let dy = player.y - g.y;
            let distance = dx.hypot(dy);
            let d = if distance == 0.0 { 1.0 } else { distance };
            let speed = view.scale * if sweeping { 18.0 } else { 9.0 };
            g.vx = dx / d * speed;
            g.vy = dy / d * speed;
            g.life = g.life.max(1.0);
        }
        g.x += g.vx * dt;
        g.y += g.vy * dt;
        if player.magnet <= 0.0 && !sweeping {
            let k = 1.0 - (dt * 2.5).min(1.0);
            g.vx *= k;
            g.vy *= k;
        }
        // JS clamp is max(lo,min(hi,value)); unlike f64::clamp it permits inverted bounds.
        g.x = g.r.max((view.width - g.r).min(g.x));
        g.y = (view.top_limit + g.r).max((view.ground_y - g.r).min(g.y));
        if g.life <= 0.0 {
            gems.remove(i);
            continue;
        }
        let got = (player.x - g.x).hypot(player.y - g.y) < reach + g.r
            || followers
                .iter()
                .any(|w| (w[0] - g.x).hypot(w[1] - g.y) < view.scale * 0.34 + g.r);
        if got {
            let g = gems.remove(i);
            if !g.lost_one {
                collect(g);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_drops_do_not_collect_but_sweep_rescues_them() {
        let p = Player {
            x: 100.0,
            y: 200.0,
            ..Player::default()
        };
        let view = Viewport::from_screen(1000.0, 700.0, 1.0, 40.0, 0.62);
        let gem = Gem {
            x: p.x,
            y: p.y,
            r: 8.0,
            life: 0.01,
            value: 3.0,
            ..Gem::default()
        };
        let mut n = 0;
        update_gems(&mut vec![gem.clone()], 0.02, &p, view, false, &[], |_| {
            n += 1
        });
        assert_eq!(n, 0);
        update_gems(&mut vec![gem], 0.02, &p, view, true, &[], |_| n += 1);
        assert_eq!(n, 1);
    }
    #[test]
    fn lost_gems_are_removed_without_reward_and_followers_collect() {
        let p = Player {
            x: 100.0,
            y: 200.0,
            ..Player::default()
        };
        let view = Viewport::from_screen(1000.0, 700.0, 1.0, 40.0, 0.62);
        let mut gems = vec![
            Gem {
                x: 400.0,
                y: 200.0,
                r: 8.0,
                life: 1.0,
                value: 1.0,
                ..Gem::default()
            },
            Gem {
                x: p.x,
                y: p.y,
                r: 8.0,
                life: 1.0,
                lost_one: true,
                ..Gem::default()
            },
        ];
        let mut n = 0;
        update_gems(&mut gems, 0.0, &p, view, false, &[[400.0, 200.0]], |_| {
            n += 1
        });
        assert_eq!(n, 1);
        assert!(gems.is_empty());
    }
}
