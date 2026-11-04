//! Static, phone-sized puppet inspection; no game/browser automation.
fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/queen-triggerfish-poses.png".into());
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="900" height="360"><rect width="900" height="360" fill="#173942"/>"##,
    );
    for panel in 0..3 {
        let mut items = crabrig::triggerfish::build();
        if panel == 0 {
            let angle = -0.9_f64;
            for item in items.iter_mut().filter(|i| i.id.starts_with("spine")) {
                for p in &mut item.pts {
                    let x = p[0] - 58.0;
                    let y = p[1] + 117.0;
                    *p = [
                        58.0 + x * angle.cos() - y * angle.sin(),
                        -117.0 + x * angle.sin() + y * angle.cos(),
                    ];
                }
            }
        }
        if panel > 0 {
            let angle = 0.38_f64;
            let rotate = |p: [f64; 2]| {
                let x = p[0] - 140.0;
                let y = p[1] + 45.0;
                [
                    140.0 + x * angle.cos() - y * angle.sin(),
                    -45.0 + x * angle.sin() + y * angle.cos(),
                ]
            };
            for item in &mut items {
                item.pts = item.pts.iter().map(|&p| rotate(p)).collect();
                item.pv = item.pv.map(rotate);
            }
        }
        if panel == 2 {
            for n in -1..=1 {
                let angle = 1.49 + f64::from(n) * 20.0_f64.to_radians();
                items.push(
                    crabrig::item::Item::new(
                        crabrig::item::Kind::Line,
                        format!("jet-{n}"),
                        None,
                        vec![
                            [140.0, -45.0],
                            [140.0 + 180.0 * angle.cos(), -45.0 + 180.0 * angle.sin()],
                        ],
                        "triggerBlue",
                    )
                    .sw(4.0)
                    .op(0.55),
                );
            }
        }
        let mut sheet = crabrig::svg::SvgOut::new(800.0, 800.0, None);
        let posed = crabrig::item::transform_items(
            &items,
            crabrig::geom::chain(&[crabrig::geom::tr(300.0, 520.0), crabrig::geom::sc(2.0)]),
        );
        sheet.items(
            &posed,
            &crabrig::palette::normal(),
            &crabrig::svg::ItemsOpt {
                scale: 2.0,
                ..Default::default()
            },
        );
        let sheet = sheet
            .to_string()
            .replacen("<svg ", &format!("<svg x=\"{}\" y=\"0\" ", panel * 300), 1)
            .replacen("width=\"800\"", "width=\"300\"", 1)
            .replacen("height=\"800\"", "height=\"360\"", 1);
        svg.push_str(&sheet);
    }
    svg.push_str("</svg>");
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
    let mut pix = resvg::tiny_skia::Pixmap::new(900, 360).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pix.as_mut(),
    );
    pix.save_png(out).unwrap();
}
