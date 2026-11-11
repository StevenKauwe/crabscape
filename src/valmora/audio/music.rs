use super::{Note, Wave, tone};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicScene {
    Silent,
    Calm,
    Realm(usize),
}
struct Theme {
    bpm: f64,
    root: f64,
    scale: &'static [i32],
    chords: [&'static [i32]; 4],
    lead: Wave,
    bass: Wave,
    drums: bool,
    pad: f64,
    step: u64,
    bell: bool,
}
fn theme(scene: MusicScene) -> Theme {
    use Wave::*;
    let (bpm, root, scale, chords, lead, bass, drums, pad, step, bell): (
        _,
        _,
        &[i32],
        [&[i32]; 4],
        _,
        _,
        _,
        _,
        _,
        _,
    ) = match scene {
        MusicScene::Realm(0) => (
            104.,
            62.,
            &[0, 2, 4, 7, 9],
            [&[0, 4, 7], &[7, 11, 14], &[9, 12, 16], &[5, 9, 12]],
            Triangle,
            Triangle,
            false,
            0.04,
            1,
            false,
        ),
        MusicScene::Realm(1) => (
            96.,
            57.,
            &[0, 3, 5, 7, 10],
            [&[0, 3, 7], &[-2, 2, 5], &[3, 7, 10], &[-4, 0, 3]],
            Sine,
            Triangle,
            false,
            0.045,
            1,
            false,
        ),
        MusicScene::Realm(2) => (
            92.,
            67.,
            &[0, 2, 4, 7, 9, 11],
            [&[0, 4, 7], &[4, 7, 11], &[5, 9, 12], &[2, 5, 9]],
            Sine,
            Sine,
            false,
            0.05,
            2,
            true,
        ),
        MusicScene::Realm(3) => (
            110.,
            60.,
            &[0, 2, 4, 5, 7, 9],
            [&[0, 4, 7], &[5, 9, 12], &[2, 5, 9], &[7, 11, 14]],
            Triangle,
            Triangle,
            true,
            0.04,
            1,
            false,
        ),
        MusicScene::Realm(4) => (
            128.,
            55.,
            &[0, 2, 3, 7, 8],
            [&[0, 3, 7], &[-4, 0, 3], &[-2, 2, 5], &[0, 3, 7]],
            Sawtooth,
            Square,
            true,
            0.03,
            1,
            false,
        ),
        MusicScene::Realm(5) => (
            80.,
            64.,
            &[0, 2, 4, 7, 9, 11],
            [&[0, 4, 7, 11], &[5, 9, 12], &[-3, 0, 4, 7], &[7, 11, 14]],
            Sine,
            Sine,
            false,
            0.06,
            2,
            true,
        ),
        _ => (
            84.,
            60.,
            &[0, 2, 4, 7, 9],
            [&[0, 4, 7], &[-3, 0, 4], &[5, 9, 12], &[7, 11, 14]],
            Triangle,
            Sine,
            false,
            0.05,
            2,
            false,
        ),
    };
    Theme {
        bpm,
        root,
        scale,
        chords,
        lead,
        bass,
        drums,
        pad,
        step,
        bell,
    }
}
/// Randomness is supplied by the presentation, never consumes the simulation RNG.
#[derive(Default)]
pub struct Music {
    key: Option<MusicScene>,
    beat: u64,
    next: f64,
    degree: i32,
}
impl Music {
    pub fn tick(
        &mut self,
        now: f64,
        scene: MusicScene,
        boss: bool,
        mut random: impl FnMut() -> f64,
    ) -> Vec<Note> {
        if scene == MusicScene::Silent {
            self.key = None;
            return vec![];
        }
        if self.key != Some(scene) {
            self.key = Some(scene);
            self.beat = 0;
            self.next = self.next.max(now + 0.15);
            self.degree = 2;
        }
        let m = theme(scene);
        let spb = 60. / (m.bpm * if boss { 1.12 } else { 1. }) / 2.;
        // Discard missed scheduling time after suspension rather than allocate unbounded catch-up audio.
        if self.next < now - 0.4 {
            self.next = now;
        }
        let mut out = Vec::new();
        while self.next < now + 0.4 {
            let b = self.beat;
            let chord = m.chords[(b / 8 % 4) as usize];
            let on = b.is_multiple_of(2);
            let delay = (self.next - now).max(0.);
            let mut note = |midi: f64, d, w, v| {
                let mut n = tone(440. * 2_f64.powf((midi - 69.) / 12.), d, w, v, None, delay);
                n.music = true;
                out.push(n);
            };
            if b.is_multiple_of(8) {
                for c in chord {
                    note(m.root + f64::from(*c), spb * 8. * 0.95, Wave::Sine, m.pad);
                }
            }
            if b.is_multiple_of(4) || boss && on {
                note(m.root - 12. + f64::from(chord[0]), spb * 1.6, m.bass, 0.12);
            }
            if b.is_multiple_of(m.step) && (on || random() < 0.7) {
                let steps = [
                    -2,
                    -1,
                    -1,
                    0,
                    1,
                    1,
                    2,
                    if b.is_multiple_of(8) { 0 } else { 3 },
                ];
                self.degree = (self.degree + steps[(random() * 8.) as usize % 8])
                    .clamp(0, m.scale.len() as i32 * 2 - 1);
                let degree = self.degree as usize;
                let midi = m.root
                    + 12. * (degree / m.scale.len()) as f64
                    + f64::from(m.scale[degree % m.scale.len()]);
                note(
                    midi,
                    spb * if m.bell { 2.6 } else { 1.1 },
                    m.lead,
                    if m.lead == Wave::Sawtooth {
                        0.035
                    } else {
                        0.07
                    },
                );
                if m.bell {
                    note(midi + 12., spb * 2., Wave::Sine, 0.025);
                }
            }
            if m.drums || boss {
                let drum = match b % 4 {
                    0 => Some((220., 0.08, 0.16)),
                    2 => Some((1800., 0.12, 0.08)),
                    _ => Some((6000., 0.05, 0.03)),
                };
                if let Some((f, d, v)) = drum {
                    let mut n = tone(
                        f,
                        d,
                        if f < 500. {
                            Wave::Lowpass
                        } else {
                            Wave::Bandpass
                        },
                        v,
                        None,
                        delay,
                    );
                    n.music = true;
                    out.push(n);
                }
            }
            self.next += spb;
            self.beat += 1;
        }
        out
    }
}
