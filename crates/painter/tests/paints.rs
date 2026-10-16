//! The painter paints the same picture every time, and paints where it is asked to.

use painter::{Painting, PhotoOptions, Tool, brush_fill, brush_stroke, brush_wash, oil};

fn px(img: &[u8], x: usize, y: usize) -> &[u8] {
    &img[(y * oil::WI + x) * 3..(y * oil::WI + x) * 3 + 3]
}

fn hash(px: &[u8]) -> u64 {
    px.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

/// The brush port (fill, wash, ink stroke) and an oil shape, on one canvas.
fn brushwork() -> Vec<u8> {
    let mut p = Painting::new();
    let blob: Vec<[f32; 2]> = (0..24)
        .map(|i| {
            let a = i as f32 / 24.0 * std::f32::consts::TAU;
            [400.0 + 120.0 * a.cos(), 400.0 + 80.0 * a.sin()]
        })
        .collect();
    brush_wash(&mut p, &blob, [0.85, 0.45, 0.30], 0.8);
    brush_fill(&mut p, &blob, [0.75, 0.25, 0.18], 0.7, 0.1, 0.4, 0.35, 7);
    brush_stroke(&mut p, &blob, true, [0.16, 0.08, 0.06], 4.0, 11);
    let square = [[900.0, 300.0], [1100.0, 300.0], [1100.0, 500.0], [900.0, 500.0]];
    oil::paint_body(&mut p, &square, [0.2, 0.4, 0.6], [0.3, 0.5, 0.7], 0.97, 3, 1.0, 1.0, Tool::Filbert);
    p.lit()
}

#[test]
fn brushwork_is_deterministic_and_stays_in_place() {
    let (a, b) = (brushwork(), brushwork());
    assert_eq!(hash(&a), hash(&b), "same strokes, same picture");
    let blank = Painting::new().lit();
    // painted inside the shapes...
    assert_ne!(px(&a, 400, 400), px(&blank, 400, 400));
    assert_ne!(px(&a, 1000, 400), px(&blank, 1000, 400));
    // ...and nothing far away from them
    assert_eq!(px(&a, 1600, 900), px(&blank, 1600, 900));
    assert_eq!(px(&a, 100, 1000), px(&blank, 100, 1000));
}

/// Paints two full 1920x1080 canvases (about 15 s on 16 cores): not run by CI. `cargo test -p painter -- --ignored`
#[test]
#[ignore]
fn a_small_picture_paints_the_same_twice() {
    let dir = std::env::temp_dir().join(format!("crabpaint-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // a tiny 2-colour picture with a soft disc in it (the painter scales it up to its canvas)
    let (w, h) = (64u32, 36u32);
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        let d = ((x as f32 - 40.0).hypot(y as f32 - 18.0) / 10.0).min(1.0);
        image::Rgb([(200.0 - 120.0 * (1.0 - d)) as u8, (120.0 + 60.0 * (1.0 - d)) as u8, 80])
    });
    let input = dir.join("in.png");
    img.save(&input).unwrap();
    let opts = PhotoOptions { attention: false, ground_subject: true, ..PhotoOptions::default() };
    let mut outs = vec![];
    for n in 0..2 {
        let out = dir.join(format!("out-{n}.png"));
        let args = [input.to_string_lossy().into_owned(), out.to_string_lossy().into_owned()];
        painter::run_photo_with(&args, &opts).unwrap();
        outs.push(image::open(&out).unwrap().to_rgb8());
    }
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(outs[0].dimensions(), (1920, 1080));
    assert_eq!(hash(outs[0].as_raw()), hash(outs[1].as_raw()));
}
