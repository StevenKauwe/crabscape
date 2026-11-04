//! Compact circulation from a streamfunction, rather than a faded vertical force.
//! With psi = -amplitude * horizontal * vertical, velocity is (psi_y, -psi_x).
//! Mixed derivatives cancel: divergence is zero and every cross-section's
//! rising flux equals its return flux. The support boundary has zero velocity.
use bevy_math::{Rect, Vec2};

fn smooth(t: f32) -> (f32, f32) {
    let t = t.clamp(0.0, 1.0);
    (
        t * t * t * (10.0 + t * (-15.0 + 6.0 * t)),
        30.0 * t * t * (1.0 - t) * (1.0 - t),
    )
}

pub(super) fn circulation(
    p: Vec2,
    bounds: Rect,
    split: f32,
    speed: f32,
    bottom: f32,
    top: f32,
) -> Vec2 {
    if !bounds.contains(p) {
        return Vec2::ZERO;
    }
    let width = split - bounds.min.x;
    // A broad, full-speed core keeps the escape lane usable near its wall.
    // Integrating the smooth-edged velocity gives the streamfunction: tapering
    // velocity alone would lose mass at the ends.
    let ramp = width.min(60.0) * 0.5;
    let integral = |t: f32| t.powi(4) * (2.5 - 3.0 * t + t * t);
    let distance = p.x - bounds.min.x;
    let (h, dx) = if p.x <= split {
        let (area, speed) = if distance < ramp {
            let t = distance / ramp;
            (ramp * integral(t), smooth(t).0)
        } else if distance > width - ramp {
            let t = (width - distance) / ramp;
            (width - ramp - ramp * integral(t), smooth(t).0)
        } else {
            (distance - ramp * 0.5, 1.0)
        };
        (area / (width - ramp), speed / (width - ramp))
    } else {
        let w = bounds.max.x - split;
        let (s, d) = smooth((p.x - split) / w);
        (1.0 - s, -d / w)
    };
    let (b, db) = smooth((p.y - bounds.min.y) / bottom);
    let (t, dt) = smooth((bounds.max.y - p.y) / top);
    let amplitude = speed * (width - ramp);
    Vec2::new(
        -amplitude * h * (db / bottom * t - b * dt / top),
        amplitude * dx * b * t,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const BOX: Rect = Rect {
        min: Vec2::new(6152.0, -9400.0),
        max: Vec2::new(6850.0, -2650.0),
    };
    fn flow(p: Vec2) -> Vec2 {
        circulation(p, BOX, 6400.0, 800.0, 120.0, 600.0)
    }
    #[test]
    fn rising_flux_returns_and_boundaries_are_still() {
        for y in [-9390.0, -9000.0, -3200.0, -3000.0, -2651.0] {
            let flux: f32 = (0..698)
                .map(|i| flow(Vec2::new(6152.5 + i as f32, y)).y)
                .sum();
            assert!(flux.abs() < 1.0, "unbalanced flux {flux} at {y}");
        }
        for i in 0..100 {
            let t = i as f32 / 99.0;
            for p in [
                Vec2::new(6152.0, -9400.0 + t * 6750.0),
                Vec2::new(6850.0, -9400.0 + t * 6750.0),
                Vec2::new(6152.0 + t * 698.0, -9400.0),
                Vec2::new(6152.0 + t * 698.0, -2650.0),
            ] {
                assert!(flow(p).length() < 0.01, "boundary flow {p}: {:?}", flow(p));
            }
        }
    }
    #[test]
    fn authored_loop_does_not_flow_through_canyon_rock() {
        let mut level = crate::Level::test_level();
        level.currents.clear();
        let tide = crate::Tuning::base().tide;
        for x in (6100..6900).step_by(20) {
            for y in (-9500..-2500).step_by(40) {
                let p = Vec2::new(x as f32, y as f32);
                if p.y <= level.floor_height(p.x) || level.solids().any(|r| r.contains(p)) {
                    assert!(
                        level.water_velocity(p, &tide).length() < 0.01,
                        "flow through rock at {p}"
                    );
                }
            }
        }
    }

    #[test]
    fn turns_are_continuous_and_locally_divergence_free() {
        for x in (6160..6850).step_by(37) {
            for y in (-9390..-2650).step_by(79) {
                let p = Vec2::new(x as f32, y as f32);
                let dx = (flow(p + Vec2::X) - flow(p - Vec2::X)) * 0.5;
                let dy = (flow(p + Vec2::Y) - flow(p - Vec2::Y)) * 0.5;
                assert!(
                    (dx.x + dy.y).abs() < 0.02,
                    "divergence at {p}: {}",
                    dx.x + dy.y
                );
            }
        }
        for x in [6200.0, 6276.0, 6400.0, 6600.0] {
            let before = flow(Vec2::new(x, -3150.1));
            let after = flow(Vec2::new(x, -3149.9));
            assert!((after - before).length() < 1.0, "old ledge remains");
        }
        assert!(flow(Vec2::new(6400.0, -2900.0)).x > 0.0);
        assert!(flow(Vec2::new(6600.0, -5000.0)).y < 0.0);
    }
}
