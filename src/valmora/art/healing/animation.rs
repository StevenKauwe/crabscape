use super::*;
#[allow(clippy::too_many_arguments)]
fn puff(
    c: &mut Canvas,
    x: f64,
    y: f64,
    col: &str,
    n: usize,
    t0: f64,
    size: f64,
    u: f64,
    time: f64,
) {
    c.fill_style(col);
    for k in 0..n {
        let t = (time * 0.25 + k as f64 / n as f64 + t0) % 1.;
        c.alpha(0.45 * (1. - t));
        circle(
            c,
            x + (t * 6. + k as f64).sin() * 2. * u,
            y - t * 16. * u * size,
            (1.5 + t * 4.) * u * size,
        );
    }
    c.alpha(1.);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn animated(
    c: &mut Canvas,
    size: [f64; 2],
    done: &BTreeSet<String>,
    st: HealingCraft,
    wild: &[&serde_json::Value],
    flash: Option<HealFlash>,
    time: f64,
) {
    let [w, h] = size;
    let u = h / 100.;
    let f = done.len() as f64 / 18.;
    let x = |v: f64| v * w;
    let y = |v: f64| v * h;
    let has = |id: &str| done.contains(id);
    for k in 0..4 {
        let k = k as f64;
        let (cx, cy) = (
            x(((time * (0.006 + k * 0.002) + k * 0.31) % 1.3) - 0.15),
            y(0.1 + k * 0.06),
        );
        c.fill_style(if f > 0.4 {
            format!("rgba(255,250,240,{})", 0.35 + 0.3 * f)
        } else {
            "rgba(120,122,136,0.45)".into()
        });
        for [ox, oy, r] in [
            [0., 0., 3.],
            [3., 1., 2.4],
            [-3., 1., 2.2],
            [1.4, -1.6, 2.2],
        ] {
            circle(c, cx + ox * u, cy + oy * u, r * u);
        }
    }
    c.stroke_style("rgba(255,255,255,0.7)");
    c.width(1_f64.max(0.35 * u));
    c.dash(&[1.2 * u, 2.4 * u]);
    c.dash_offset(-time * 14.);
    for [ix, iy, r] in ISLES.into_iter().take(2) {
        let (fx, fy) = (x(ix) - r * w * 0.55, y(iy) + r * w * 0.2);
        c.fill_style(if f > 0.3 {
            "rgba(200,236,255,0.45)"
        } else {
            "rgba(150,150,160,0.35)"
        });
        c.fill_rect(fx - 0.5 * u, fy, u, y(0.12));
        c.begin();
        c.move_to(fx, fy);
        c.line_to(fx, fy + y(0.12));
        c.stroke();
    }
    c.begin();
    c.move_to(x(0.465) + 0.6 * u, y(0.58));
    c.line_to(x(0.465) + 0.6 * u, y(0.68));
    c.stroke();
    c.dash(&[]);
    for i in 0..26 {
        let (px, py, ph) = (
            0.63 + ((i * 37) % 100) as f64 / 100. * 0.35,
            0.78 + ((i * 53) % 100) as f64 / 100. * 0.2,
            i as f64 * 1.7,
        );
        let a = (time * 2. + ph).sin().max(0.) * (0.25 + 0.6 * f);
        if a < 0.05 {
            continue;
        }
        c.alpha(a);
        draw_star(c, x(px), y(py), 0.9 * u, "#ffffff", ph);
    }
    c.alpha(1.);
    if has("stream") {
        for k in 0..4 {
            let t = (time * 0.15 + k as f64 / 4.) % 1.;
            c.alpha((t * PI).sin());
            draw_star(
                c,
                x(0.47 + 0.17 * t),
                y(0.7 + 0.14 * t),
                0.7 * u,
                "#ffffff",
                t * 6.,
            );
        }
    }
    c.alpha(1.);
    puff(c, x(0.8), y(0.38), "#6a5a5a", 4, 0., 1., u, time);
    if has("spring") {
        puff(c, x(0.88), y(0.7), "#ffffff", 3, 0.3, 0.7, u, time);
    }
    if st.saddles || st.armor > 0. || st.bellows > 0. {
        puff(
            c,
            x(0.5) + 4.2 * u,
            y(0.88) - 3.2 * u,
            "#8a8a90",
            3,
            0.6,
            0.6,
            u,
            time,
        );
    }
    if has("lanterns") {
        for [px, k] in [[0.15, 0.], [0.3, 0.5]] {
            puff(
                c,
                x(px) + 1.4 * u,
                y(0.845) - 6.2 * u,
                "#d8d4dc",
                2,
                k,
                0.4,
                u,
                time,
            );
        }
    }
    if has("barns") {
        c.save();
        c.translate(x(0.035), y(0.8) - 10.4 * u);
        c.rotate(time * 0.8);
        for _ in 0..4 {
            c.rotate(TAU / 4.);
            c.fill_style("#f4ecd8");
            c.fill_rect(-0.5 * u, 0., u, 7. * u);
            c.stroke_style("#8a6a4a");
            c.width(1_f64.max(0.2 * u));
            stroke_rect(c, -0.5 * u, 0., u, 7. * u);
        }
        c.fill_style("#6a4a2a");
        circle(c, 0., 0., 0.7 * u);
        c.restore();
    }
    if has("bell") {
        let (fx, fy) = (x(0.25), y(0.81) - 23. * u);
        c.stroke_style("#5a4a3a");
        c.width(1_f64.max(0.25 * u));
        c.begin();
        c.move_to(fx, fy);
        c.line_to(fx, fy - 4. * u);
        c.stroke();
        c.fill_style("#ffcf3f");
        c.begin();
        c.move_to(fx, fy - 4. * u);
        for k in 0..=6 {
            let k = k as f64;
            c.line_to(
                fx + k * 0.6 * u,
                fy - 4. * u + (time * 6. + k).sin() * 0.4 * u,
            );
        }
        for k in (0..=6).rev() {
            let k = k as f64;
            c.line_to(
                fx + k * 0.6 * u,
                fy - 2.2 * u + (time * 6. + k).sin() * 0.4 * u,
            );
        }
        c.close();
        c.fill();
    }
    if has("lanterns") {
        for [px, py] in LANTERNS {
            c.fill_style("#3a2f28");
            c.fill_rect(x(px) - 0.3 * u, y(py), 0.6 * u, 4. * u);
            glow(
                c,
                x(px),
                y(py),
                (2.6 + (time * 5. + px * 40.).sin() * 0.4) * u,
                "#ffd76e",
                0.75,
            );
        }
    }
    if has("beacon") {
        glow(
            c,
            x(0.38),
            y(0.2) - 5. * u,
            (5. + (time * 7.).sin()) * u,
            "#ffb347",
            0.75,
        );
        c.fill_style("#ffe27a");
        c.begin();
        c.move_to(x(0.38) - 1.2 * u, y(0.2) - 3.8 * u);
        c.quad(
            x(0.38),
            y(0.2) - (8. + (time * 9.).sin()) * u,
            x(0.38) + 1.2 * u,
            y(0.2) - 3.8 * u,
        );
        c.fill();
    }
    if has("lighthouse") {
        let a = PI - 0.2 + (time * 0.6).sin() * 0.14;
        let (lx, ly, len) = (x(0.945), y(0.48), x(0.34));
        c.fill_style("rgba(255,240,180,0.24)");
        c.begin();
        c.move_to(lx, ly);
        c.line_to(lx + (a - 0.06).cos() * len, ly + (a - 0.06).sin() * len);
        c.line_to(lx + (a + 0.06).cos() * len, ly + (a + 0.06).sin() * len);
        c.close();
        c.fill();
        glow(c, lx, ly, 3. * u, "#fff1a8", 0.75);
    }
    if has("oaks") {
        for k in 0..2 {
            let k = k as f64;
            let (bx, by) = (x(((time * 0.02 + k * 0.5) % 1.2) - 0.1), y(0.3 + k * 0.07));
            for [ox, oy] in [[0., 0.], [-2.4, 1.2], [-2.4, -1.2], [-4.6, 2.2]] {
                let fl = (time * 10. + ox).sin() * 0.6 * u;
                c.stroke_style("#2a2a3a");
                c.width(1_f64.max(0.28 * u));
                c.begin();
                c.move_to(bx + ox * u - u, by + oy * u - fl);
                c.line_to(bx + ox * u, by + oy * u);
                c.line_to(bx + ox * u + u, by + oy * u - fl);
                c.stroke();
            }
        }
    }
    if has("meadow") {
        for k in 0..4 {
            let (bx, by, fl) = (
                x(0.36 + (time * 0.7 + k as f64 * 2.).sin() * 0.04),
                y(0.68 + (time * 1.3 + k as f64).sin() * 0.02),
                (time * 14. + k as f64).sin().abs() * 0.8 * u + 0.2 * u,
            );
            c.fill_style(["#ffb0d8", "#9fd8ff", "#ffe27a", "#c9a8ff"][k]);
            c.begin();
            c.ellipse(bx - fl * 0.6, by, fl, 0.7 * u, -0.4, 0., TAU);
            c.ellipse(bx + fl * 0.6, by, fl, 0.7 * u, 0.4, 0., TAU);
            c.fill();
        }
    }
    if has("oaks") && f < 0.8 {
        for k in 0..6 {
            let k = k as f64;
            let (fx, fy) = (
                x(0.06 + k * 0.06 + (time * 0.5 + k).sin() * 0.01),
                y(0.6 + (time * 0.8 + k * 2.).sin() * 0.03),
            );
            c.alpha(0.5 + 0.5 * (time * 3. + k).sin());
            glow(c, fx, fy, 1.2 * u, "#fff27a", 0.9);
        }
    }
    c.alpha(1.);
    for (i, d) in wild.iter().skip(wild.len().saturating_sub(6)).enumerate() {
        let i = i as f64;
        let a = time * (0.18 + i * 0.02) + i * 1.9;
        dragon_game::drawing::draw_dragon(
            c,
            x(0.5 + 0.42 * a.sin()),
            y(0.26 + 0.14 * (a * 1.7 + i).sin()),
            3.4 * u,
            d["color"].as_str().unwrap_or("#ff6a3d"),
            if a.cos() >= 0. { 1. } else { -1. },
            time * 1.2 + i,
            &dragon_game::drawing::DragonArt {
                horn: Some(super::super::horn(d)),
                ..Default::default()
            },
        );
    }
    super::super::scenes::wick(c, x(0.43), y(0.8), 3.6 * u, time);
    if let Some(flash) = flash {
        let t = time - flash.started;
        if t <= 3. {
            for k in 0..10 {
                let a = k as f64 / 10. * TAU + t;
                let r = (3. + t * 5.) * u;
                draw_star(
                    c,
                    x(flash.spot[0]) + a.cos() * r,
                    y(flash.spot[1]) + a.sin() * r * 0.7,
                    u * 1.3,
                    "#fff1a8",
                    t * 3.,
                );
            }
        }
    }
}
