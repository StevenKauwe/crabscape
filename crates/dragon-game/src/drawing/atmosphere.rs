//! Direct Canvas ports of drawMoonRays and drawForeground in web/dragon/index.html.
use super::{Canvas, circle, hash01};
use crate::flight::Viewport;
use std::f64::consts::{PI, TAU};

pub fn draw_moon_rays(c: &mut Canvas, v: Viewport, time: f64) {
    c.save();
    c.additive(true);
    c.translate(
        v.width * 0.84,
        (v.height * 0.24).max(v.top_limit + v.scale * 1.4),
    );
    c.rotate(time * 0.03);
    for i in 0..5 {
        c.rotate(TAU / 5.);
        c.alpha(0.035 + 0.02 * (time * 0.7 + i as f64).sin());
        c.fill_style("#eaf2ff");
        c.begin();
        c.move_to(0., 0.);
        c.line_to(v.width * 1.2, -v.scale * 1.2);
        c.line_to(v.width * 1.2, v.scale * 1.6);
        c.close();
        c.fill();
    }
    c.restore();
}

pub fn draw_foreground(c: &mut Canvas, v: Viewport, realm: usize, scroll: f64, time: f64) {
    let (w, h, s, ground) = (v.width, v.height, v.scale, v.ground_y);
    let sp = s * 1.7;
    let i0 = (scroll * 1.35 / sp).floor() as i64 - 1;
    let i1 = i0 + (w / sp).ceil() as i64 + 3;
    let gy = ground + s * 0.35;
    c.save();
    c.fill_style(
        [
            "#060c20", "#030d0b", "#a8c4e4", "#041624", "#120806", "#cfc6ea",
        ][realm.min(5)],
    );
    for i in i0..i1 {
        let i = i as f64;
        if hash01(i, 7.) < 0.45 {
            continue;
        }
        let x = i * sp - scroll * 1.35 + hash01(i, 8.) * sp * 0.5;
        let k = s * (0.35 + hash01(i, 9.) * 0.45);
        let kind = hash01(i, 10.);
        if realm == 3 {
            c.begin();
            c.move_to(x - k * 1.6, gy + s * 0.2);
            c.quad(x - k * 0.6, gy - k * 0.5, x, gy + s * 0.05);
            c.quad(x + k * 0.7, gy - k * 0.35, x + k * 1.7, gy + s * 0.2);
            c.line_to(x + k * 1.7, h);
            c.line_to(x - k * 1.6, h);
            c.close();
            c.fill();
        } else if realm == 5 {
            circle(c, x, gy, k * 0.9);
            circle(c, x + k * 0.8, gy + k * 0.2, k * 0.7);
        } else if realm == 2 && kind < 0.5 {
            c.begin();
            c.ellipse(x, gy, k * 1.1, k * 0.6, 0., PI, TAU);
            c.fill();
        } else if kind < 0.5 {
            for b in -2..=2 {
                let b = b as f64;
                let bx = x + b * k * 0.22;
                let bh = k * (0.9 + hash01(i, 11. + b) * 0.8);
                c.begin();
                c.move_to(bx - k * 0.08, gy);
                c.quad(bx + k * 0.1, gy - bh * 0.6, bx + b * k * 0.1, gy - bh);
                c.line_to(bx + k * 0.08, gy);
                c.close();
                c.fill();
            }
        } else {
            circle(c, x, gy - k * 0.2, k * 0.75);
            circle(c, x + k * 0.7, gy - k * 0.05, k * 0.6);
            circle(c, x - k * 0.6, gy, k * 0.5);
        }
    }
    c.fill_rect(0., ground + s * 0.35, w, h);
    if realm == 4 {
        c.additive(true);
        for i in i0..i1 {
            let i = i as f64;
            if hash01(i, 12.) < 0.6 {
                continue;
            }
            let x = i * sp - scroll * 1.35 + hash01(i, 13.) * sp;
            let gl = c.radial(
                [x, ground + s * 0.6, 0., x, ground + s * 0.6, s * 1.2],
                &[
                    (
                        0.,
                        format!("rgba(255,120,40,{})", 0.25 + 0.1 * (time * 4. + i).sin()),
                    ),
                    (1., "rgba(255,120,40,0)".into()),
                ],
            );
            c.fill_style(gl);
            c.fill_rect(x - s * 1.2, ground, s * 2.4, h - ground);
        }
    }
    c.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{Paint, PathCommand};

    #[test]
    fn moon_rays_keep_source_geometry_and_restore_canvas() {
        let v = Viewport::from_screen(1200., 800., 1., 50., 1.);
        let mut c = Canvas::default();
        draw_moon_rays(&mut c, v, 2.);
        assert_eq!(c.commands.len(), 5);
        for (i, d) in c.commands.iter().enumerate() {
            assert!(d.additive);
            assert_eq!(
                d.path,
                vec![
                    PathCommand::Move(0., 0.),
                    PathCommand::Line(v.width * 1.2, -v.scale * 1.2),
                    PathCommand::Line(v.width * 1.2, v.scale * 1.6),
                    PathCommand::Close
                ]
            );
            assert_eq!(d.alpha, 0.035 + 0.02 * (1.4 + i as f64).sin());
            assert_eq!(
                d.transform.point(0., 0.),
                [
                    v.width * 0.84,
                    (v.height * 0.24).max(v.top_limit + v.scale * 1.4)
                ]
            );
        }
        c.fill_rect(0., 0., 1., 1.);
        let d = c.commands.last().expect("probe");
        assert!(!d.additive);
        assert_eq!(d.alpha, 1.);
        assert_eq!(d.transform, crate::drawing::Affine::default());
    }

    #[test]
    fn foreground_is_deterministic_and_viewport_bounded_in_all_realms() {
        let v = Viewport::from_screen(1200., 800., 1., 50., 1.);
        for realm in 0..6 {
            let mut c = Canvas::default();
            draw_foreground(&mut c, v, realm, 12345., 2.);
            let mut same = Canvas::default();
            draw_foreground(&mut same, v, realm, 12345., 2.);
            assert_eq!(c.commands, same.commands);
            assert!(c.commands.len() > 1 && c.commands.len() < 120);
            assert_eq!(
                c.commands
                    .iter()
                    .any(|d| matches!(d.paint, Paint::Radial { .. })),
                realm == 4
            );
            c.fill_rect(0., 0., 1., 1.);
            assert!(!c.commands.last().expect("probe").additive);
        }
    }
}

/// Ground illumination from at most eight nearby shots, as in drawLights.
pub fn draw_shot_lights(c: &mut Canvas, v: Viewport, shots: &[crate::combat::Shot]) {
    use crate::combat::Kind;
    c.save();
    c.additive(true);
    for sh in shots
        .iter()
        .filter(|sh| sh.y >= v.ground_y - v.scale * 2.6)
        .take(8)
    {
        let color = match sh.kind {
            Kind::Flame => "255,150,60",
            Kind::Bond | Kind::Hammer => "255,170,80",
            Kind::Spark => "255,235,120",
            Kind::Frost => "120,200,255",
            Kind::Star => "255,225,140",
            Kind::Arrow => "200,180,255",
            _ => "255,180,90",
        };
        let k = 1. - (v.ground_y - sh.y) / (v.scale * 2.6);
        let gl = c.radial(
            [sh.x, v.ground_y, 0., sh.x, v.ground_y, v.scale * 1.5],
            &[
                (0., format!("rgba({color},{})", 0.22 * k)),
                (1., format!("rgba({color},0)")),
            ],
        );
        c.fill_style(gl);
        super::ell(c, sh.x, v.ground_y, v.scale * 1.5, v.scale * 0.5);
    }
    c.restore();
}

#[cfg(test)]
mod light_tests {
    use super::*;
    #[test]
    fn ground_lights_skip_high_shots_and_cap_nearby_lights_at_eight() {
        let v = Viewport::from_screen(1200., 800., 1., 50., 1.);
        let mut shots = vec![
            crate::combat::Shot {
                y: 0.,
                ..Default::default()
            };
            5
        ];
        shots.extend((0..20).map(|_| crate::combat::Shot {
            y: v.ground_y,
            ..Default::default()
        }));
        let mut c = Canvas::default();
        draw_shot_lights(&mut c, v, &shots);
        assert_eq!(c.commands.len(), 8);
        assert!(c.commands.iter().all(|d| d.additive));
    }
}
