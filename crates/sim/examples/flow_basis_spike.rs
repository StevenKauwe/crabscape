//! Isolated background-wave benchmark, reproducing the new factored algebra.
//! This measures wave generation only, never the complete gameplay kernel.
//! The matching production implementation is in underwater/background_waves.rs.
use crabsim::underwater::{HEIGHT, WIDTH};
use std::{hint::black_box, time::Instant};

type Pair = (f32, f32);
fn sin_cos(x: f32) -> Pair {
    (libm::sinf(x), libm::cosf(x))
}
fn add(a: Pair, b: Pair) -> f32 {
    a.0 * b.1 + a.1 * b.0
}
fn reference(origin: (i32, i32), tick: u64, out: &mut [Pair]) {
    let t = tick as f32 * 0.018;
    for y in 0..HEIGHT {
        let wy = (origin.1 + y as i32) as f32;
        for x in 0..WIDTH {
            let wx = (origin.0 + x as i32) as f32;
            out[y * WIDTH + x] = (
                libm::sinf(wx * 0.047 + wy * 0.023 + t),
                libm::sinf(wx * 0.027 - wy * 0.041 - t * 0.7),
            );
        }
    }
}
struct Basis(Vec<(Pair, Pair)>);
impl Basis {
    fn new(origin_x: i32) -> Self {
        Self(
            (0..WIDTH)
                .map(|x| {
                    let wx = (origin_x + x as i32) as f32;
                    (sin_cos(wx * 0.047), sin_cos(wx * 0.027))
                })
                .collect(),
        )
    }
    fn write(&self, origin_y: i32, tick: u64, out: &mut [Pair]) {
        let t = tick as f32 * 0.018;
        for (y, row) in out.as_chunks_mut::<WIDTH>().0.iter_mut().enumerate() {
            let wy = (origin_y + y as i32) as f32;
            let a = sin_cos(wy * 0.023 + t);
            let b = sin_cos(-wy * 0.041 - t * 0.7);
            for (pixel, &(x1, x2)) in row.iter_mut().zip(&self.0) {
                *pixel = (add(x1, a), add(x2, b));
            }
        }
    }
}
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("run --release");
        std::process::exit(2);
    }
    for origin in [(0, 0), (3000, -1500), (249_000, -249_000)] {
        let basis = Basis::new(origin.0);
        let mut a = vec![(0.0, 0.0); WIDTH * HEIGHT];
        let mut b = a.clone();
        for tick in [0, 9000, 108_000, 1_000_000] {
            reference(origin, tick, &mut a);
            basis.write(origin.1, tick, &mut b);
            let mut max_error = 0.0_f32;
            let mut squared = 0.0_f64;
            for (&x, &y) in a.iter().zip(&b) {
                for e in [(x.0 - y.0).abs(), (x.1 - y.1).abs()] {
                    max_error = max_error.max(e);
                    squared += f64::from(e).powi(2);
                }
            }
            println!(
                "origin={origin:?} tick={tick}: wave max error={max_error:.7}, rms={:.7}",
                (squared / (2 * WIDTH * HEIGHT) as f64).sqrt()
            );
        }
        let started = Instant::now();
        for tick in 9000..9060 {
            reference(origin, tick, black_box(&mut a));
            black_box(&a);
        }
        let reference_ms = started.elapsed().as_secs_f64() * 1000.0 / 60.0;
        let started = Instant::now();
        for tick in 9000..9060 {
            basis.write(origin.1, tick, black_box(&mut b));
            black_box(&b);
        }
        let basis_ms = started.elapsed().as_secs_f64() * 1000.0 / 60.0;
        println!(
            "background waves ONLY: original={reference_ms:.3}ms basis={basis_ms:.3}ms ratio={:.1}x; excludes local diffusion, materials, crossing, rendering",
            reference_ms / basis_ms
        );
    }
}
