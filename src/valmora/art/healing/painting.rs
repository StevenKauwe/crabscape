use super::*;
pub(super) fn static_paint(
    c: &mut Canvas,
    size: [f64; 2],
    done: &BTreeSet<String>,
    st: HealingCraft,
    random: &mut impl FnMut() -> f64,
) {
    let [w, h] = size;
    let u = h / 100.;
    let f = done.len() as f64 / 18.;
    let has = |id: &str| done.contains(id);
    let x = |v: f64| v * w;
    let y = |v: f64| v * h;
    let m = |a: &str, b: &str| mix(a, b, f);
    let mut rng = dragon_game::random::Mulberry32 { state: 7 };
    let mut rnd = || rng.next_unit();
    let p = lin(
        c,
        size,
        [0., 0., 0., 0.78],
        &[
            (0., m("#1e2236", "#2a4f9e")),
            (0.45, m("#3b3e52", "#6f8fd0")),
            (0.78, m("#5a5a66", "#f7b98a")),
            (1., m("#6a6670", "#ffe2b0")),
        ],
    );
    c.fill_style(p);
    c.fill_rect(0., 0., w, h);
    for _ in 0..50 {
        c.fill_style(format!("rgba(255,250,230,{})", (0.7 - 0.6 * f) * rnd()));
        c.fill_rect(x(rnd()), y(rnd() * 0.32), 1.3, 1.3);
    }
    let (sx, sy, sr) = (x(0.58), y(0.5 - 0.3 * f), 6. * u);
    glow(c, sx, sy, sr * 6., "#ffd890", 0.15 + 0.45 * f);
    if f > 0.3 {
        c.save();
        c.translate(sx, sy);
        c.alpha((f - 0.3) * 0.5);
        c.fill_style("#fff3c8");
        for _ in 0..14 {
            c.rotate(TAU / 14.);
            tri(c, -sr * 0.12, sr * 1.3, 0., sr * 5., sr * 0.12, sr * 1.3);
        }
        c.restore();
    }
    let p = c.radial(
        [sx, sy, 0., sx, sy, sr],
        &[(0., "#fffbe8".into()), (1., m("#bfbfc8", "#ffd27a"))],
    );
    c.alpha(0.35 + 0.65 * f);
    c.fill_style(p);
    circle(c, sx, sy, sr);
    c.alpha(1.);
    if f < 1. {
        for _ in 0..9 {
            c.fill_style(format!("rgba(70,72,86,{})", 0.35 * (1. - f)));
            let (cx, cy) = (x(rnd()), y(0.08 + rnd() * 0.2));
            for j in 0..4 {
                circle(
                    c,
                    cx + (j as f64 - 1.5) * 4. * u,
                    cy + (j as f64 * 2.).sin() * u,
                    (4. + rnd() * 3.) * u,
                );
            }
        }
    }
    for [ix, iy, r] in ISLES {
        let (rr, cx, cy) = (r * w, x(ix), y(iy));
        let p = c.linear(
            [cx, cy, cx, cy + rr * 1.2],
            &[(0., m("#4a4a5e", "#7a6ac0")), (1., m("#22222e", "#2e2a5a"))],
        );
        c.fill_style(p);
        c.begin();
        c.move_to(cx - rr, cy);
        c.quad(
            cx - rr * 0.65,
            cy + rr * 0.45,
            cx - rr * 0.25,
            cy + rr * 0.7,
        );
        c.quad(cx, cy + rr * 1.3, cx + rr * 0.22, cy + rr * 0.68);
        c.quad(cx + rr * 0.7, cy + rr * 0.45, cx + rr, cy);
        c.close();
        c.fill();
        c.stroke_style("rgba(0,0,0,0.18)");
        c.width(1.);
        for k in 0..3 {
            let k = k as f64;
            c.begin();
            c.move_to(cx - rr * (0.7 - k * 0.3), cy + rr * 0.15);
            c.line_to(cx - rr * (0.5 - k * 0.32), cy + rr * (0.45 + k * 0.12));
            c.stroke();
        }
        c.fill_style(m("#55554f", "#5fa878"));
        ell(c, cx, cy, rr, rr * 0.18);
        c.fill_style(m("#6a6a62", "#8fd49a"));
        ell(c, cx - rr * 0.15, cy - rr * 0.04, rr * 0.7, rr * 0.09);
        c.fill_style(m("#3a3530", "#5a3f28"));
        c.fill_rect(cx + rr * 0.35, cy - rr * 0.32, rr * 0.06, rr * 0.3);
        c.fill_style(m("#4a4a44", "#3f8f55"));
        circle(c, cx + rr * 0.38, cy - rr * 0.38, rr * 0.18);
        c.fill_style(m("#5a5a52", "#5fb070"));
        circle(c, cx + rr * 0.33, cy - rr * 0.43, rr * 0.08);
        if has("flowers") {
            for k in 0..9 {
                let fx = cx + (k as f64 - 4.) * rr * 0.2;
                let fy = cy - rr * 0.06;
                glow(c, fx, fy, 1.6 * u, "#fff0ff", 0.5);
                c.fill_style(if k % 2 != 0 { "#fff1ff" } else { "#ffc4ec" });
                circle(c, fx, fy, 1.3_f64.max(0.55 * u));
            }
        }
    }
    if has("bridges") {
        for i in 0..2 {
            let (a, b) = (ISLES[i], ISLES[i + 1]);
            let (x0, x1, y0, y1) = (
                x(a[0] + a[2] * 0.85),
                x(b[0] - b[2] * 0.85),
                y(a[1]),
                y(b[1]),
            );
            let my = y0.max(y1) + 4. * u;
            c.stroke_style("#c8b48a");
            c.width(1.2_f64.max(0.35 * u));
            for off in [0., -1.6 * u] {
                c.begin();
                c.move_to(x0, y0 + off);
                c.quad((x0 + x1) / 2., my + off, x1, y1 + off);
                c.stroke();
            }
            c.fill_style("#9a7a52");
            for k in 1..10 {
                let t = k as f64 / 10.;
                let px = (1. - t) * (1. - t) * x0 + 2. * t * (1. - t) * (x0 + x1) / 2. + t * t * x1;
                let py = (1. - t) * (1. - t) * y0 + 2. * t * (1. - t) * my + t * t * y1;
                c.fill_rect(px - 0.4 * u, py - 0.2 * u, 0.8 * u, 0.6 * u);
            }
        }
    }
    if has("star") {
        glow(c, x(0.82), y(0.07), 11. * u, "#fff1a8", 0.85);
        draw_star(c, x(0.82), y(0.07), 3.6 * u, "#fff1a8", 0.);
        draw_star(c, x(0.82), y(0.07), 1.6 * u, "#ffffff", 0.4);
    }
    if f > 0.75 {
        c.save();
        c.alpha((f - 0.75) * 4. * 0.45);
        c.width(2_f64.max(1.3 * u));
        for (i, col) in [
            "#ff5a5a", "#ffa64a", "#ffe45a", "#6ad46a", "#5ab4ff", "#9a7aff",
        ]
        .iter()
        .enumerate()
        {
            c.stroke_style(*col);
            c.begin();
            c.arc(
                x(0.24),
                y(0.78),
                x(0.22) - i as f64 * 1.3 * u,
                PI * 1.04,
                PI * 1.96,
            );
            c.stroke();
        }
        c.restore();
    }
    c.fill_style(m("#3a3c50", "#8ea2d2"));
    c.begin();
    c.move_to(x(0.18), y(0.72));
    let mut xx = 0.18;
    while xx <= 0.7 {
        c.line_to(
            x(xx),
            y(0.5 + rnd() * 0.12 - ((xx - 0.18) * 6.).sin() * 0.06),
        );
        xx += 0.035;
    }
    c.line_to(x(0.7), y(0.72));
    c.close();
    c.fill();
    for [px, top, half] in [
        [0.6, 0.42, 0.1],
        [0.28, 0.34, 0.1],
        [0.5, 0.3, 0.12],
        [0.38, 0.2, 0.13],
    ] {
        let base = 0.72;
        let sh = (base - top) * 0.3;
        poly(
            c,
            size,
            &[[px - half, base], [px, top], [px + 0.01, base]],
            m("#4a4d62", "#7f9ad0"),
        );
        poly(
            c,
            size,
            &[[px, top], [px + half, base], [px + 0.01, base]],
            m("#2e3044", "#4a62a0"),
        );
        poly(
            c,
            size,
            &[
                [px, top],
                [px - half * 0.32, top + sh],
                [px - half * 0.18, top + sh * 0.8],
                [px - half * 0.07, top + sh * 1.05],
                [px, top + sh * 0.82],
            ],
            m("#8a8d98", "#ffffff"),
        );
        poly(
            c,
            size,
            &[
                [px, top],
                [px, top + sh * 0.82],
                [px + half * 0.1, top + sh * 1.1],
                [px + half * 0.2, top + sh * 0.85],
                [px + half * 0.32, top + sh],
            ],
            m("#6a6d78", "#c8daf5"),
        );
    }
    let p = lin(
        c,
        size,
        [0., 0.58, 0., 0.74],
        &[
            (0., "rgba(0,0,0,0)".into()),
            (
                1.,
                if f > 0.5 {
                    format!("rgba(255,214,170,{})", 0.35 * f)
                } else {
                    format!("rgba(120,120,132,{})", 0.35 * (1. - f))
                },
            ),
        ],
    );
    c.fill_style(p);
    c.fill_rect(x(0.15), y(0.58), x(0.5), y(0.16));
    for k in 0..22 {
        let px = x(0.22 + k as f64 * 0.019);
        let py = y(0.71 + rnd() * 0.02);
        let ph = (2. + rnd() * 2.) * u;
        c.fill_style(m("#2c3530", "#2f6a4a"));
        tri(c, px - ph * 0.35, py, px, py - ph, px + ph * 0.35, py);
    }
    c.fill_style(if has("stream") {
        "rgba(200,236,255,0.85)"
    } else {
        "rgba(150,150,160,0.6)"
    });
    c.fill_rect(x(0.465), y(0.58), 1.2 * u, y(0.1));
    if has("bridge") {
        c.fill_style("#c4b8a2");
        c.fill_rect(x(0.4), y(0.42), x(0.09), 1.4 * u);
        c.fill_style("#a89c86");
        for xx in [0.405, 0.44, 0.475] {
            c.fill_rect(x(xx), y(0.42) + 1.4 * u, 1.6 * u, 6. * u);
        }
        c.stroke_style("#8a7e6a");
        c.width(1_f64.max(0.3 * u));
        c.begin();
        for xx in [0.415, 0.45] {
            c.move_to(x(xx) + 1.2 * u, y(0.42) + 7.4 * u);
            c.quad(
                x(xx) + 4. * u,
                y(0.42) + 2.4 * u,
                x(xx + 0.035) - 0.4 * u,
                y(0.42) + 7.4 * u,
            );
        }
        c.stroke();
    } else {
        c.fill_style("#6a665e");
        c.fill_rect(x(0.4), y(0.42), x(0.028), 1.4 * u);
        c.fill_rect(x(0.462), y(0.42), x(0.028), 1.4 * u);
        c.fill_style("#4a4640");
        tri(
            c,
            x(0.428),
            y(0.42),
            x(0.425),
            y(0.42) + 3. * u,
            x(0.432),
            y(0.42) + 1.4 * u,
        );
    }
    let (hx, hy, hall) = (x(0.5), y(0.45), has("hall"));
    c.fill_style(if hall { "#b8aa92" } else { "#4a4a52" });
    c.fill_rect(hx, hy, 8. * u, 6.5 * u);
    c.fill_style(if hall { "#8a3a34" } else { "#34343c" });
    tri(
        c,
        hx - 1.2 * u,
        hy,
        hx + 4. * u,
        hy - 4. * u,
        hx + 9.2 * u,
        hy,
    );
    c.fill_style(if hall { "#d8ccb4" } else { "#5a5a62" });
    for k in [0.6, 3.4, 6.2] {
        c.fill_rect(hx + k * u, hy + 0.6 * u, 0.9 * u, 5.9 * u);
    }
    c.fill_style(if hall { "#ffd76e" } else { "#202028" });
    c.begin();
    c.move_to(hx + 3. * u, hy + 6.5 * u);
    c.line_to(hx + 3. * u, hy + 3.4 * u);
    c.arc(hx + 4. * u, hy + 3.4 * u, u, PI, TAU);
    c.line_to(hx + 5. * u, hy + 6.5 * u);
    c.close();
    c.fill();
    if hall {
        glow(c, hx + 4. * u, hy + 4.5 * u, 5. * u, "#ffd76e", 0.35);
    }
    if has("beacon") {
        c.fill_style("#8a8478");
        c.fill_rect(x(0.38) - 1.1 * u, y(0.2) - 3.4 * u, 2.2 * u, 3.4 * u);
        c.fill_style("#5a5248");
        c.fill_rect(x(0.38) - 1.6 * u, y(0.2) - 3.8 * u, 3.2 * u, 0.6 * u);
    }
    poly(
        c,
        size,
        &[[0.66, 0.74], [0.78, 0.38], [0.8, 0.39], [0.79, 0.74]],
        m("#3e3436", "#8a5642"),
    );
    poly(
        c,
        size,
        &[[0.79, 0.74], [0.8, 0.39], [0.82, 0.38], [0.97, 0.74]],
        m("#2a2426", "#5a3424"),
    );
    glow(c, x(0.8), y(0.385), 6. * u, "#ff8a3d", 0.55);
    c.fill_style("#ffb347");
    ell(c, x(0.8), y(0.385), x(0.02), 0.9 * u);
    c.stroke_style("rgba(255,120,40,0.85)");
    c.width(1.2_f64.max(0.45 * u));
    for dx in [-0.01, 0.012] {
        c.begin();
        let (mut px, mut py) = (x(0.8 + dx), y(0.39));
        c.move_to(px, py);
        while py < y(0.7) {
            py += 3. * u;
            px += (rnd() - 0.45 + dx * 30.) * 1.6 * u;
            c.line_to(px, py);
        }
        c.stroke();
    }
    if has("path") {
        for k in 0..10 {
            let t = k as f64 / 9.;
            let (px, py) = (
                x(0.7 + 0.1 * t + (t * 9.).sin() * 0.015),
                y(0.72 - 0.32 * t),
            );
            glow(c, px, py, 2.2 * u, "#ffd76e", 0.5);
            c.fill_style("#fff1a8");
            circle(c, px, py, 1.3_f64.max(0.5 * u));
        }
    }
    if has("spring") {
        c.fill_style("#6a5a52");
        ell(c, x(0.88), y(0.712), 6.4 * u, 2.3 * u);
        let p = c.radial(
            [x(0.88), y(0.71), 0., x(0.88), y(0.71), 5. * u],
            &[(0., "#d8fbff".into()), (1., "#5fc8d8".into())],
        );
        c.fill_style(p);
        ell(c, x(0.88), y(0.71), 5.2 * u, 1.7 * u);
    }
    if has("garden") {
        for _ in 0..12 {
            let (px, py) = (x(0.67 + rnd() * 0.07), y(0.67 + rnd() * 0.06));
            let col = ["#ffb347", "#ff6fb5", "#ffe27a"][(random() * 3.).floor() as usize];
            c.fill_style("#3f7a4a");
            c.fill_rect(px - 0.15 * u, py, 0.3 * u, 1.6 * u);
            glow(c, px, py, 2. * u, col, 0.45);
            c.fill_style(col);
            for p in 0..5 {
                let a = p as f64 / 5. * TAU;
                circle(
                    c,
                    px + a.cos() * 0.5 * u,
                    py + a.sin() * 0.5 * u,
                    0.8_f64.max(0.4 * u),
                );
            }
        }
    }
    let sea_t = 0.3 * f
        + 0.7
            * ["lighthouse", "harbor", "pools"]
                .iter()
                .filter(|id| has(id))
                .count() as f64
            / 3.;
    let p = lin(
        c,
        size,
        [0., 0.74, 0., 1.],
        &[
            (0., mix("#5a6470", "#8ad4ee", sea_t)),
            (0.4, mix("#3a4650", "#2e86c0", sea_t)),
            (1., mix("#252e38", "#164a7a", sea_t)),
        ],
    );
    c.fill_style(p);
    c.begin();
    c.move_to(x(0.6), y(0.76));
    c.quad(x(0.8), y(0.72), x(1.), y(0.74));
    c.line_to(x(1.), y(1.));
    c.line_to(x(0.6), y(1.));
    c.close();
    c.fill();
    c.stroke_style(format!("rgba(235,250,255,{})", 0.18 + 0.2 * sea_t));
    c.width(1_f64.max(0.3 * u));
    for _ in 0..22 {
        let (wx, wy, ww) = (
            x(0.62 + rnd() * 0.37),
            y(0.78 + rnd() * 0.2),
            (1. + rnd() * 2.) * u,
        );
        c.begin();
        c.arc(wx, wy, ww, 1.15 * PI, 1.85 * PI);
        c.stroke();
    }
    let p = lin(
        c,
        size,
        [0.9, 0.62, 1., 0.84],
        &[(0., m("#44444c", "#6a5a4a")), (1., m("#2a2a30", "#3a3028"))],
    );
    poly(
        c,
        size,
        &[
            [0.89, 0.86],
            [0.905, 0.72],
            [0.93, 0.66],
            [1., 0.62],
            [1., 0.86],
        ],
        p,
    );
    poly(
        c,
        size,
        &[[0.925, 0.665], [1., 0.62], [1., 0.64], [0.93, 0.675]],
        m("#55554f", "#5fa878"),
    );
    let lit = has("lighthouse");
    let p = lin(
        c,
        size,
        [0.93, 0., 0.96, 0.],
        &[
            (0., if lit { "#fff8ec" } else { "#7a7674" }.into()),
            (1., if lit { "#d8ccba" } else { "#55524f" }.into()),
        ],
    );
    poly(
        c,
        size,
        &[[0.93, 0.665], [0.936, 0.5], [0.954, 0.5], [0.96, 0.665]],
        p,
    );
    if lit {
        c.fill_style("#c0453f");
        for yy in [0.53, 0.6] {
            c.fill_rect(x(0.933), y(yy), x(0.024), 2.2 * u);
        }
    }
    c.fill_style("#2a2a3a");
    c.fill_rect(x(0.932), y(0.495), x(0.026), 0.7 * u);
    c.fill_style(if lit { "#ffd76e" } else { "#2a2a30" });
    c.fill_rect(x(0.937), y(0.46), x(0.016), 3.4 * u);
    c.fill_style("#2a2a3a");
    tri(c, x(0.931), y(0.46), x(0.945), y(0.41), x(0.959), y(0.46));
    if has("harbor") {
        c.fill_style("#7a5a3a");
        c.fill_rect(x(0.66), y(0.84), x(0.12), 1.3 * u);
        c.fill_style("#5a4028");
        for k in 0..6 {
            c.fill_rect(x(0.665 + k as f64 * 0.022), y(0.84), 0.6 * u, 3.4 * u);
        }
        for (px, py, col) in [
            (0.72, 0.895, "#c0553f"),
            (0.83, 0.87, "#3f7ac0"),
            (0.9, 0.94, "#e8a83f"),
        ] {
            c.fill_style(col);
            c.begin();
            c.move_to(x(px) - 3.4 * u, y(py));
            c.line_to(x(px) + 3.4 * u, y(py));
            c.line_to(x(px) + 2.2 * u, y(py) + 2.2 * u);
            c.line_to(x(px) - 2.2 * u, y(py) + 2.2 * u);
            c.close();
            c.fill();
            c.stroke_style("#4a3a2a");
            c.width(1_f64.max(0.3 * u));
            c.begin();
            c.move_to(x(px), y(py));
            c.line_to(x(px), y(py) - 7. * u);
            c.stroke();
            c.fill_style("#fff6e0");
            tri(
                c,
                x(px) + 0.3 * u,
                y(py) - 6.6 * u,
                x(px) + 0.3 * u,
                y(py) - 0.6 * u,
                x(px) + 3.6 * u,
                y(py) - 0.6 * u,
            );
            c.fill_style("#ff5a6a");
            tri(
                c,
                x(px),
                y(py) - 7. * u,
                x(px),
                y(py) - 6. * u,
                x(px) - 1.6 * u,
                y(py) - 6.5 * u,
            );
        }
    }
    if has("pools") {
        for (px, py, col) in [
            (0.625, 0.93, "#ff6fb5"),
            (0.65, 0.965, "#ffd76e"),
            (0.675, 0.94, "#62f0d4"),
        ] {
            c.fill_style("#6a6a72");
            ell(c, x(px), y(py), 3. * u, 1.3 * u);
            c.fill_style("#7fd8ec");
            ell(c, x(px), y(py), 2.3 * u, 0.9 * u);
            draw_star(c, x(px) - 0.6 * u, y(py), 0.8 * u, col, 0.3);
            c.fill_style("#ff8a5a");
            circle(c, x(px) + 0.9 * u, y(py) + 0.1 * u, 0.4 * u);
        }
    }
    let oaks = has("oaks");
    let p = lin(
        c,
        size,
        [0., 0.5, 0., 0.8],
        &[
            (0., m("#5a5a50", if oaks { "#5fb070" } else { "#7a7a64" })),
            (1., m("#3a3a34", if oaks { "#2f7a48" } else { "#4e5040" })),
        ],
    );
    c.fill_style(p);
    c.begin();
    c.move_to(0., y(0.68));
    c.quad(x(0.2), y(0.48), x(0.42), y(0.64));
    c.quad(x(0.52), y(0.7), x(0.6), y(0.76));
    c.line_to(x(0.6), y(1.));
    c.line_to(0., y(1.));
    c.close();
    c.fill();
    let hill_y = |px: f64| 0.68 - 0.13 * ((px / 0.42).min(1.) * PI * 0.95).sin();
    for k in 0..18 {
        let px = 0.01 + k as f64 * 0.024;
        let py = hill_y(px) + 0.01;
        let (px, py, r) = (x(px), y(py), (1.8 + rnd()) * u);
        if oaks {
            c.fill_style(m("#3a4a3a", "#2a6a40"));
            circle(c, px, py - r, r);
        } else {
            c.stroke_style("#3e3a36");
            c.width(1_f64.max(0.35 * u));
            c.begin();
            c.move_to(px, py);
            c.line_to(px, py - r * 1.6);
            c.stroke();
        }
    }
    for k in 0..14 {
        let xx = 0.025 + k as f64 * 0.03 + rnd() * 0.01;
        let yy = hill_y(xx) + 0.04 + rnd() * 0.03;
        let (px, py, t) = (x(xx), y(yy), (3. + rnd() * 1.4) * u);
        c.fill_style("rgba(0,0,0,0.18)");
        ell(c, px + 0.8 * u, py + 0.3 * u, 2. * u, 0.6 * u);
        c.stroke_style(if oaks { "#5a3f28" } else { "#3a3530" });
        c.width(1.2_f64.max(0.55 * u));
        c.begin();
        c.move_to(px, py);
        c.line_to(px, py - t);
        c.stroke();
        if oaks {
            let cr = (2. + rnd() * 0.8) * u;
            let p = c.radial(
                [
                    px - cr * 0.4,
                    py - t - cr * 0.6,
                    cr * 0.2,
                    px,
                    py - t,
                    cr * 1.3,
                ],
                &[
                    (0., if k % 3 != 0 { "#7fd08a" } else { "#9ae0a0" }.into()),
                    (1., if k % 3 != 0 { "#2a7040" } else { "#357a4a" }.into()),
                ],
            );
            c.fill_style(p);
            for [ox, oy, rr] in [[0., -0.2, 1.], [-0.7, 0.3, 0.75], [0.7, 0.3, 0.75]] {
                circle(c, px + ox * cr, py - t + oy * cr, cr * rr);
            }
        } else {
            c.begin();
            c.move_to(px, py - t * 0.55);
            c.line_to(px - t * 0.38, py - t * 0.95);
            c.move_to(px, py - t * 0.7);
            c.line_to(px + t * 0.4, py - t * 1.1);
            c.move_to(px - t * 0.2, py - t * 0.78);
            c.line_to(px - t * 0.32, py - t * 1.12);
            c.stroke();
        }
    }
    let stream_path = |c: &mut Canvas| {
        c.begin();
        c.move_to(x(0.47), y(0.68));
        c.quad(x(0.44), y(0.8), x(0.54), y(0.8));
        c.quad(x(0.6), y(0.8), x(0.64), y(0.84));
    };
    c.cap("round");
    c.stroke_style(m("#4a4640", "#7a6a4a"));
    c.width(3_f64.max(2.4 * u));
    stream_path(c);
    c.stroke();
    c.stroke_style(if has("stream") { "#5ab4ec" } else { "#6a6f78" });
    c.width(2_f64.max(1.6 * u));
    stream_path(c);
    c.stroke();
    c.stroke_style(if has("stream") {
        "rgba(220,250,255,0.7)"
    } else {
        "rgba(180,180,190,0.35)"
    });
    c.width(1_f64.max(0.35 * u));
    c.dash(&[2. * u, 3. * u]);
    stream_path(c);
    c.stroke();
    c.dash(&[]);
    if has("meadow") {
        for k in 0..30 {
            let (px, py) = (x(0.31 + rnd() * 0.12), y(0.69 + rnd() * 0.07));
            c.fill_style("#4f9a5a");
            c.fill_rect(px - 0.1 * u, py, 0.2 * u, 0.9 * u);
            c.fill_style(["#ffd76e", "#ff9fcf", "#ffffff", "#c9a8ff"][k % 4]);
            circle(c, px, py, 1_f64.max(0.45 * u));
        }
        for [px, py, s] in [[0.35, 0.715, 1.], [0.39, 0.74, 0.85], [0.335, 0.745, 0.6]] {
            let (dx, dy, k) = (x(px), y(py), s * u);
            c.fill_style("#b07a4a");
            ell(c, dx, dy, 1.9 * k, 1.05 * k);
            c.fill_style("#c8945e");
            ell(c, dx + 2. * k, dy - 1.1 * k, 0.85 * k, 0.75 * k);
            c.fill_style("#f4e4cc");
            ell(c, dx - 1.6 * k, dy - 0.4 * k, 0.4 * k, 0.35 * k);
            c.fill_style("#7a5434");
            for lx in [-1.3, -0.6, 0.6, 1.3] {
                c.fill_rect(dx + lx * k, dy + 0.4 * k, 0.35 * k, 1.8 * k);
            }
            c.stroke_style("#7a5434");
            c.width(1_f64.max(0.25 * k));
            c.begin();
            c.move_to(dx + 2.1 * k, dy - 1.8 * k);
            c.line_to(dx + 1.8 * k, dy - 2.6 * k);
            c.move_to(dx + 2.4 * k, dy - 1.8 * k);
            c.line_to(dx + 2.7 * k, dy - 2.6 * k);
            c.stroke();
        }
    }
    let p = lin(
        c,
        size,
        [0., 0.76, 0., 1.],
        &[(0., m("#4a4844", "#6aa860")), (1., m("#33312e", "#3f7a40"))],
    );
    c.fill_style(p);
    c.begin();
    c.move_to(0., y(0.8));
    c.quad(x(0.3), y(0.74), x(0.6), y(0.88));
    c.line_to(x(0.6), y(1.));
    c.line_to(0., y(1.));
    c.close();
    c.fill();
    c.stroke_style(m("#6a5e50", "#c8a878"));
    c.width(3_f64.max(2.2 * u));
    c.begin();
    c.move_to(x(0.18), y(1.));
    c.quad(x(0.22), y(0.9), x(0.25), y(0.82));
    c.stroke();
    c.stroke_style(m("#4a4440", "#a88a60"));
    c.width(1_f64.max(0.3 * u));
    for k in 0..9 {
        let px = x(0.06 + k as f64 * 0.035);
        c.begin();
        c.move_to(px, y(0.9));
        c.line_to(px, y(0.9) - 1.6 * u);
        c.stroke();
    }
    c.begin();
    c.move_to(x(0.06), y(0.9) - 1.1 * u);
    c.line_to(x(0.34), y(0.9) - 1.1 * u);
    c.stroke();
    if has("barns") {
        c.fill_style("#d8ccb4");
        c.begin();
        c.move_to(x(0.035) - 1.6 * u, y(0.8));
        c.line_to(x(0.035) - 0.9 * u, y(0.8) - 10. * u);
        c.line_to(x(0.035) + 0.9 * u, y(0.8) - 10. * u);
        c.line_to(x(0.035) + 1.6 * u, y(0.8));
        c.close();
        c.fill();
        c.fill_style("#8a3a34");
        tri(
            c,
            x(0.035) - 1.3 * u,
            y(0.8) - 10. * u,
            x(0.035),
            y(0.8) - 12. * u,
            x(0.035) + 1.3 * u,
            y(0.8) - 10. * u,
        );
        c.fill_style("#b5443a");
        c.fill_rect(x(0.02), y(0.8), 6.4 * u, 5.6 * u);
        c.fill_style("#7a2f2a");
        tri(
            c,
            x(0.02) - u,
            y(0.8),
            x(0.02) + 3.2 * u,
            y(0.8) - 3.6 * u,
            x(0.02) + 7.4 * u,
            y(0.8),
        );
        c.stroke_style("#fff3e0");
        c.width(1_f64.max(0.3 * u));
        stroke_rect(c, x(0.02) + 2. * u, y(0.8) + 2.4 * u, 2.4 * u, 3.2 * u);
        c.begin();
        c.move_to(x(0.02) + 2. * u, y(0.8) + 2.4 * u);
        c.line_to(x(0.02) + 4.4 * u, y(0.8) + 5.6 * u);
        c.move_to(x(0.02) + 4.4 * u, y(0.8) + 2.4 * u);
        c.line_to(x(0.02) + 2. * u, y(0.8) + 5.6 * u);
        c.stroke();
        c.fill_style("#e8c86a");
        for px in [0.078, 0.098] {
            c.begin();
            c.arc(x(px), y(0.865), 1.9 * u, PI, TAU);
            c.fill();
            c.stroke_style("#c8a84a");
            c.begin();
            c.arc(x(px), y(0.865), 1.2 * u, PI * 1.1, PI * 1.9);
            c.stroke();
        }
    } else {
        c.stroke_style("#4e4640");
        c.width(1.5_f64.max(0.7 * u));
        for [a, b] in [[-3., 3.], [-1., 4.], [1., 2.], [3., -2.], [-2., 1.]] {
            c.begin();
            c.move_to(x(0.05) + a * u, y(0.865));
            c.line_to(x(0.05) + b * u, y(0.81));
            c.stroke();
        }
    }
    let tower = has("bell");
    c.save();
    c.translate(x(0.25), y(0.81));
    c.rotate(if tower { 0. } else { 0.16 });
    let p = lin(
        c,
        size,
        [-0.003, 0., 0.004, 0.],
        &[
            (0., if tower { "#d8c8ac" } else { "#6a6662" }.into()),
            (1., if tower { "#a89a80" } else { "#4a4642" }.into()),
        ],
    );
    c.fill_style(p);
    c.fill_rect(-2.2 * u, -17. * u, 4.4 * u, 17. * u);
    c.stroke_style("rgba(0,0,0,0.15)");
    c.width(1.);
    for k in 1..7 {
        c.begin();
        c.move_to(-2.2 * u, -(k as f64) * 2.4 * u);
        c.line_to(2.2 * u, -(k as f64) * 2.4 * u);
        c.stroke();
    }
    c.fill_style("#2a2420");
    c.begin();
    c.move_to(-1.2 * u, -11. * u);
    c.line_to(-1.2 * u, -14.4 * u);
    c.arc(0., -14.4 * u, 1.2 * u, PI, TAU);
    c.line_to(1.2 * u, -11. * u);
    c.close();
    c.fill();
    c.fill_style(if tower { "#ffd76e" } else { "#4a4642" });
    c.begin();
    c.move_to(-0.9 * u, -11.6 * u);
    c.quad(-0.9 * u, -14. * u, 0., -14. * u);
    c.quad(0.9 * u, -14. * u, 0.9 * u, -11.6 * u);
    c.close();
    c.fill();
    c.fill_style(if tower { "#a03f38" } else { "#3e3a3c" });
    tri(c, -3.2 * u, -17. * u, 0., -23. * u, 3.2 * u, -17. * u);
    c.fill_style("#2a2420");
    c.fill_rect(-0.9 * u, -3. * u, 1.8 * u, 3. * u);
    c.restore();
    for [px, wide] in [
        [0.1, 1.],
        [0.15, 1.15],
        [0.2, 0.95],
        [0.3, 1.1],
        [0.35, 0.9],
    ] {
        let yy = y(0.845);
        let bw = 3.4 * u * wide;
        let bh = 3.6 * u;
        let bx = x(px) - bw / 2.;
        c.fill_style(m("#5a5752", "#efe0c4"));
        c.fill_rect(bx, yy - bh, bw, bh);
        c.stroke_style(m("#3a3634", "#7a5434"));
        c.width(1_f64.max(0.28 * u));
        stroke_rect(c, bx, yy - bh, bw, bh);
        c.begin();
        c.move_to(bx, yy - bh * 0.55);
        c.line_to(bx + bw, yy - bh * 0.55);
        c.stroke();
        c.fill_style(m("#3a3638", "#b04a3a"));
        tri(
            c,
            bx - bw * 0.18,
            yy - bh,
            x(px),
            yy - bh * 1.85,
            bx + bw * 1.18,
            yy - bh,
        );
        c.stroke_style("rgba(0,0,0,0.2)");
        c.width(1.);
        for k in 1..3 {
            let k = k as f64;
            let yy = yy - bh - k * bh * 0.27;
            c.begin();
            c.move_to(x(px) - bw * (0.68 - k * 0.22), yy);
            c.line_to(x(px) + bw * (0.68 - k * 0.22), yy);
            c.stroke();
        }
        c.fill_style(m("#4a4642", "#8a7a6a"));
        c.fill_rect(bx + bw * 0.7, yy - bh * 1.75, bw * 0.16, bh * 0.5);
        c.fill_style("#5a3a24");
        c.fill_rect(bx + bw * 0.12, yy - bh * 0.5, bw * 0.24, bh * 0.5);
        let lit = has("lanterns");
        c.fill_style(if lit { "#ffd76e" } else { "#26262e" });
        c.fill_rect(bx + bw * 0.55, yy - bh * 0.85, bw * 0.3, bh * 0.28);
        if lit {
            glow(c, bx + bw * 0.7, yy - bh * 0.7, 2.4 * u, "#ffd76e", 0.35);
            c.fill_style("#ff8fb0");
            for k in 0..3 {
                circle(
                    c,
                    bx + bw * (0.6 + k as f64 * 0.1),
                    yy - bh * 0.55,
                    0.35 * u,
                );
            }
        }
    }
    let beach_path = |c: &mut Canvas| {
        c.begin();
        c.move_to(x(0.585), y(0.76));
        c.quad(x(0.6), y(0.75), x(0.612), y(0.77));
        c.quad(x(0.63), y(0.88), x(0.655), y(1.));
    };
    beach_path(c);
    c.line_to(x(0.57), y(1.));
    c.quad(x(0.6), y(0.9), x(0.585), y(0.76));
    c.close();
    let p = lin(
        c,
        size,
        [0., 0.76, 0., 1.],
        &[(0., m("#6a665c", "#f4dca0")), (1., m("#4e4a44", "#d8b878"))],
    );
    c.fill_style(p);
    c.fill();
    c.fill_style(m("#5a564e", "#c8a468"));
    for _ in 0..10 {
        circle(
            c,
            x(0.59 + rnd() * 0.03),
            y(0.8 + rnd() * 0.18),
            0.6_f64.max(0.25 * u),
        );
    }
    c.stroke_style("rgba(255,255,255,0.7)");
    c.width(1.2_f64.max(0.5 * u));
    beach_path(c);
    c.stroke();
    if f > 0.5 {
        for (px, py, col) in [(0.6, 0.86, "#ff9a8a"), (0.615, 0.94, "#fff0e0")] {
            c.fill_style(col);
            c.begin();
            c.arc(x(px), y(py), 0.8 * u, PI, TAU);
            c.fill();
        }
    }
    let rows = (st.tea + st.tonic + if st.treats_made { 1. } else { 0. }).min(6.);
    c.fill_style("#5a4634");
    c.fill_rect(x(0.4), y(0.88), x(0.075), 5. * u);
    for k in 0..6 {
        let rx = x(0.404) + k as f64 * 1.6 * u;
        if (k as f64) < rows {
            c.fill_style("#4fae5a");
            for j in 0..4 {
                circle(c, rx + 0.45 * u, y(0.89) + j as f64 * u, 0.55 * u);
            }
            c.fill_style(if k % 2 != 0 { "#ffd76e" } else { "#ff9fcf" });
            circle(c, rx + 0.45 * u, y(0.89), 0.3 * u);
        }
    }
    c.fill_style("#6a6670");
    c.fill_rect(x(0.5), y(0.88), 5.4 * u, 4.4 * u);
    c.fill_style("#4a4650");
    tri(
        c,
        x(0.5) - 0.6 * u,
        y(0.88),
        x(0.5) + 2.7 * u,
        y(0.88) - 2.2 * u,
        x(0.5) + 6. * u,
        y(0.88),
    );
    c.fill_rect(x(0.5) + 3.6 * u, y(0.88) - 3.2 * u, 1.2 * u, 3.2 * u);
    if st.saddles || st.armor > 0. || st.bellows > 0. {
        glow(
            c,
            x(0.5) + 2. * u,
            y(0.88) + 2.4 * u,
            3. * u,
            "#ff9a3d",
            0.6,
        );
        c.fill_style("#ffb347");
        c.fill_rect(x(0.5) + u, y(0.88) + 1.8 * u, 2. * u, 1.8 * u);
    }
    for k in 0..70 {
        let (px, py, ph) = (
            x(rnd() * 0.6),
            y(0.985 + rnd() * 0.02),
            (1.2 + rnd() * 1.8) * u,
        );
        c.stroke_style(m("#4a4a40", if k % 2 != 0 { "#3f8a48" } else { "#5aa860" }));
        c.width(1_f64.max(0.3 * u));
        c.begin();
        c.move_to(px, py);
        c.quad(px + 0.4 * u, py - ph * 0.6, px + (rnd() - 0.5) * u, py - ph);
        c.stroke();
    }
    if f > 0.25 {
        for k in 0..14 {
            c.fill_style(["#ff9fcf", "#ffd76e", "#ffffff"][k % 3]);
            circle(
                c,
                x(rnd() * 0.58),
                y(0.965 + rnd() * 0.03),
                1_f64.max(0.45 * u),
            );
        }
    }
    // Deliberate omission: source's full-canvas grey haze and vignette violate repository art rules.
}
