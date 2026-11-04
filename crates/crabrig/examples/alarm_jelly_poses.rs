//! Phone-scale assembled idle/alarm inspection using the ordinary rig renderer.
fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/alarm-jelly-poses.png".into());
    let mut svg = crabrig::svg::SvgOut::new(480.0, 330.0, None);
    svg.raw(r##"<rect width="480" height="330" fill="#0A1928"/>"##);
    for panel in 0..2 {
        let mut items = crabrig::alarm_jelly::build();
        if panel == 1 {
            for n in 0..8 {
                let a = n as f64 / 8.0 * std::f64::consts::TAU;
                items.push(
                    crabrig::item::Item::new(
                        crabrig::item::Kind::Shape,
                        format!("alarm-{n}"),
                        None,
                        crabrig::geom::ell(
                            a.cos() * 43.0,
                            -10.0 - a.sin() * 9.0,
                            4.0,
                            4.0,
                            16,
                            0.0,
                        ),
                        "glint",
                    )
                    .no_ink(),
                );
            }
            items.push(
                crabrig::item::Item::new(
                    crabrig::item::Kind::Line,
                    "alarm-ring",
                    None,
                    crabrig::geom::ell(0.0, -10.0, 72.0, 26.0, 48, 0.0),
                    "triggerBlue",
                )
                .sw(2.0),
            );
        }
        for item in &mut items {
            if let Some(n) = item
                .id
                .strip_prefix("trail-")
                .and_then(|s| s.parse::<usize>().ok())
            {
                item.pts = (n..=n + 1)
                    .map(|i| {
                        let f = i as f64 / 6.0;
                        [(f * 3.0).sin() * 24.0 * f, 420.0 * f]
                    })
                    .collect();
            }
        }
        let posed = crabrig::item::transform_items(
            &items,
            crabrig::geom::chain(&[
                crabrig::geom::tr(120.0 + panel as f64 * 240.0, 70.0),
                crabrig::geom::sc(0.5),
            ]),
        );
        let mut palette = crabrig::palette::normal();
        palette.insert("glint", crabrig::palette::Mat::One("#75D5FF".into()));
        svg.items(
            &posed,
            &palette,
            &crabrig::svg::ItemsOpt {
                scale: 0.5,
                ..Default::default()
            },
        );
    }
    let tree =
        resvg::usvg::Tree::from_str(&svg.to_string(), &resvg::usvg::Options::default()).unwrap();
    let mut pix = resvg::tiny_skia::Pixmap::new(480, 330).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pix.as_mut(),
    );
    pix.save_png(out).unwrap();
}
