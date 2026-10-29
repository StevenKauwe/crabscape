//! The "Calm" track: the game's first music, kept as it was.
//!
//! Soft pads hold a chord for eight beats and fade into the next one; now
//! and then a small bell-like motif rings over them, with an echo. Where
//! the crab is picks the music, at the next chord:
//!
//! - **Home** and **the shallows**: warm, fuller pads (D minor, its sixth,
//!   third and seventh chords) and the motif fairly often.
//! - **The basins**: the same warmth, lower and sparser.
//! - **The pit**: hollow pads swaying between D minor and the half-step above
//!   it, a low two-note motif and long silences.
//! - **The canyon** (and the lights on its floor): hollow pads, a cold, high
//!   motif, far apart.
//! - **A fight**: the bells stop, a slow muffled heartbeat comes in under
//!   everything and a held, uneasy low note joins the next chord.
//!
//! The only additions: its bells rest while a stinger plays, and the
//! stingers are in its key ([`mode`]).

use super::{AEOLIAN, BEAT, Danger, HIRAJOSHI, Mode, Mood, Note, PHRYGIAN, Place, Voice};
use crate::sound::Lowpass;

/// Beats per chord.
const CHORD_BEATS: u64 = 8;
/// Pads: seconds swelling in, and fading out after the chord's eight beats
/// (so each chord overlaps the next one).
const PAD_IN: f32 = 3.0;
const PAD_OUT: f32 = 4.0;
/// The note held under a fight: a tritone above low D (A flat).
const TENSION: f32 = -6.0;

/// The mode its chords and bells keep to in `place`, for stingers.
pub fn mode(place: Place) -> Mode {
    match place {
        Place::Home | Place::Shallows | Place::Basin => AEOLIAN,
        Place::Pit => PHRYGIAN,
        Place::Canyon | Place::Lights => HIRAJOSHI,
    }
}

/// It knows the lights only as the canyon.
fn heard(place: Place) -> Place {
    if place == Place::Lights {
        Place::Canyon
    } else {
        place
    }
}

/// Down in the dark: hollow pads.
fn dark(place: Place) -> bool {
    matches!(heard(place), Place::Pit | Place::Canyon)
}

/// A chord: its bass note and three above it (semitones from D3).
type Chord = (f32, [f32; 3]);

const DM9: Chord = (-12.0, [3.0, 7.0, 14.0]);
const BB_MAJ7: Chord = (-16.0, [0.0, 3.0, 7.0]);
const F_MAJ7: Chord = (-9.0, [7.0, 10.0, 14.0]);
const C_ADD9: Chord = (-14.0, [5.0, 10.0, 12.0]);
const GM9: Chord = (-7.0, [3.0, 7.0, 8.0]);
const A_SUS4: Chord = (-5.0, [0.0, 2.0, 7.0]);
/// D with no third, just fifths and a ninth: open and bare.
const D_OPEN: Chord = (-12.0, [-5.0, 2.0, 7.0]);
const EB_MAJ7: Chord = (-11.0, [5.0, 8.0, 12.0]);
const BBM9: Chord = (-16.0, [-1.0, 3.0, 10.0]);

/// Each place's four chords, in turn.
fn progression(place: Place) -> [Chord; 4] {
    match heard(place) {
        Place::Home => [BB_MAJ7, F_MAJ7, C_ADD9, DM9],
        Place::Shallows => [DM9, BB_MAJ7, F_MAJ7, C_ADD9],
        Place::Basin => [DM9, GM9, BB_MAJ7, A_SUS4],
        Place::Pit => [D_OPEN, EB_MAJ7, D_OPEN, BBM9],
        _ => [D_OPEN, BB_MAJ7, EB_MAJ7, D_OPEN],
    }
}

/// The notes the bells pick from (semitones from D3).
fn scale(place: Place) -> &'static [f32] {
    match heard(place) {
        // D minor pentatonic, an octave up.
        Place::Home | Place::Shallows => &[12.0, 15.0, 17.0, 19.0, 22.0, 24.0, 27.0],
        Place::Basin => &[7.0, 10.0, 12.0, 14.0, 15.0, 19.0, 22.0],
        // Low, with the half-step above D.
        Place::Pit => &[0.0, 1.0, 5.0, 7.0, 8.0, 12.0],
        // High and cold.
        _ => &[24.0, 26.0, 27.0, 31.0, 32.0, 36.0],
    }
}

/// The motif's shapes: (beats after it starts, step of the scale).
fn motifs(place: Place) -> &'static [&'static [(f32, usize)]] {
    match heard(place) {
        Place::Home | Place::Shallows | Place::Basin => &[
            &[(0.0, 3), (1.0, 1), (1.5, 2), (3.0, 0)],
            &[(0.0, 3), (1.0, 4), (1.5, 5), (3.0, 3)],
            &[(0.0, 0), (0.5, 1), (1.0, 3), (2.5, 2)],
        ],
        Place::Pit => &[&[(0.0, 0), (2.0, 1)], &[(0.0, 2), (2.0, 1), (4.0, 0)]],
        _ => &[&[(0.0, 5), (0.5, 4), (2.0, 2)], &[(0.0, 3), (2.5, 0)]],
    }
}

/// Chance a chord carries the motif, and chance of a lone bell on a beat.
fn bells(place: Place) -> (f32, f32) {
    match place {
        Place::Home => (0.6, 0.15),
        Place::Shallows => (0.5, 0.15),
        Place::Basin => (0.35, 0.1),
        Place::Pit => (0.2, 0.04),
        Place::Canyon | Place::Lights => (0.3, 0.06),
    }
}

/// Writes the music, a beat at a time.
pub struct Composer {
    beat: u64,
    rng: u32,
    /// The lone bells' last step of the scale (they wander from it).
    step: usize,
    /// No bells before this beat (a stinger is playing).
    quiet_until: u64,
}

impl Default for Composer {
    fn default() -> Self {
        Self {
            beat: 0,
            rng: 0x2545_F491,
            step: 2,
            quiet_until: 0,
        }
    }
}

impl Composer {
    /// 0 up to 1.
    fn roll(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    /// No bells for the next `beats` beats.
    pub fn quiet(&mut self, beats: u64) {
        self.quiet_until = self.beat + beats;
    }

    /// The next beat's notes, for `mood`.
    pub fn beat(&mut self, mood: Mood) -> Vec<Note> {
        let beat = self.beat;
        self.beat += 1;
        let place = mood.place;
        let fight = mood.danger >= Danger::Fight;
        let quiet = beat < self.quiet_until;
        let mut notes = Vec::new();
        let (motif_chance, lone_chance) = bells(place);
        let span = CHORD_BEATS as f32 * BEAT;
        if beat.is_multiple_of(CHORD_BEATS) {
            let (bass, upper) = progression(place)[(beat / CHORD_BEATS % 4) as usize];
            let voice = if dark(place) {
                Voice::CalmDark
            } else {
                Voice::CalmWarm
            };
            let pad = |semis: f32, gain: f32| Note {
                voice,
                at: 0.0,
                semis,
                gain,
                rise: PAD_IN,
                hold: span - PAD_IN,
                fall: PAD_OUT,
                echo: false,
            };
            // The bass alone; each note above twice, the second softer and a
            // few cents sharp, so the two beat gently against each other,
            // which keeps a held chord alive.
            notes.push(pad(bass, 0.1));
            for u in upper {
                notes.push(pad(u, 0.05));
                notes.push(pad(u + 0.07, 0.025));
            }
            if fight {
                // A held tritone under it: uneasy.
                notes.push(Note {
                    voice: Voice::CalmDark,
                    ..pad(TENSION, 0.06)
                });
            } else if self.roll() < motif_chance && !quiet {
                let shapes = motifs(place);
                let shape = shapes[(self.roll() * shapes.len() as f32) as usize % shapes.len()];
                let start = 1.0 + (self.roll() * 3.0).floor();
                let notes_of = scale(place);
                for &(when, step) in shape {
                    notes.push(bell(
                        (start + when) * BEAT,
                        notes_of[step.min(notes_of.len() - 1)],
                        0.12,
                    ));
                }
            }
        }
        if fight {
            notes.push(Note {
                voice: Voice::Pulse,
                at: 0.0,
                semis: 0.0,
                gain: 0.35,
                rise: 0.0,
                hold: 0.0,
                fall: 0.0,
                echo: false,
            });
        } else if self.roll() < lone_chance && !quiet {
            // A lone bell, a step or two from the last one.
            let notes_of = scale(place);
            let wander = (self.roll() * 5.0) as usize; // 0..=4: -2..=2 steps
            self.step = (self.step + wander)
                .saturating_sub(2)
                .min(notes_of.len() - 1);
            let at = if self.roll() < 0.5 { 0.0 } else { 0.5 * BEAT };
            notes.push(bell(at, notes_of[self.step], 0.07));
        }
        notes
    }
}

fn bell(at: f32, semis: f32, gain: f32) -> Note {
    Note {
        voice: Voice::Bell,
        at,
        semis,
        gain,
        rise: 0.0,
        hold: 0.0,
        fall: 0.0,
        echo: true,
    }
}

/// Its looped pads' wave: this many samples, holding this many whole waves,
/// so it loops without a seam.
pub const LOOP_SAMPLES: usize = 4096;
pub const LOOP_WAVES: usize = 16;
/// The bell as made: its pitch and length.
pub const BELL_HZ: f32 = 440.0;
pub const BELL_SECS: f32 = 3.0;

/// Its voices' samples (not yet at their final loudness): the pads and the
/// bell.
pub fn sample(voice: Voice, rate: f32) -> Vec<f32> {
    use std::f32::consts::TAU;
    match voice {
        Voice::Bell => {
            // A soft mallet on something hollow: partials (ratio, strength,
            // seconds to die away), a gentle strike, muffled.
            let partials = [
                (1.0, 1.0, 1.6),
                (2.0, 0.3, 0.8),
                (3.0, 0.1, 0.4),
                (4.07, 0.06, 0.25),
            ];
            let n = (BELL_SECS * rate) as usize;
            let mut lp = Lowpass::default();
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    let strike = (t / 0.006).min(1.0);
                    let s: f32 = partials
                        .iter()
                        .map(|&(r, a, d)| {
                            a * (TAU * BELL_HZ * r * t).sin() * (-t / (d / 3.0)).exp()
                        })
                        .sum();
                    lp.run(s * strike, 2500.0, rate)
                })
                .collect()
        }
        _ => {
            // Harmonics (number, strength), each a whole number of waves.
            let harmonics: &[(usize, f32)] = if voice == Voice::CalmWarm {
                &[
                    (1, 1.0),
                    (2, 0.45),
                    (3, 0.3),
                    (4, 0.16),
                    (5, 0.12),
                    (6, 0.06),
                    (7, 0.04),
                    (8, 0.02),
                ]
            } else {
                &[(1, 1.0), (2, 0.18), (3, 0.1), (5, 0.03)]
            };
            (0..LOOP_SAMPLES)
                .map(|i| {
                    let p = i as f32 / LOOP_SAMPLES as f32;
                    harmonics
                        .iter()
                        .map(|&(n, a)| a * (TAU * (n * LOOP_WAVES) as f32 * p + n as f32).sin())
                        .sum()
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 22050.0;

    fn mood(place: Place, fight: bool) -> Mood {
        Mood {
            place,
            danger: if fight { Danger::Fight } else { Danger::Calm },
            low_shell: false,
        }
    }

    #[test]
    fn the_same_walk_plays_the_same_music() {
        let walk = [
            mood(Place::Home, false),
            mood(Place::Basin, false),
            mood(Place::Pit, true),
        ];
        let write = || {
            let mut c = Composer::default();
            (0..120)
                .flat_map(|b| c.beat(walk[b / 40]))
                .collect::<Vec<_>>()
        };
        assert_eq!(write(), write());
    }

    #[test]
    fn chords_change_every_eight_beats_and_overlap() {
        let mut c = Composer::default();
        for b in 0..64 {
            let pads: Vec<Note> = c
                .beat(mood(Place::Home, false))
                .into_iter()
                .filter(|n| matches!(n.voice, Voice::CalmWarm | Voice::CalmDark))
                .collect();
            if b % CHORD_BEATS == 0 {
                // A bass and three notes above it, those doubled.
                assert_eq!(pads.len(), 7, "beat {b}");
                for p in &pads {
                    let len = p.length(RATE);
                    let chord = CHORD_BEATS as f32 * BEAT;
                    assert!(len > chord && len < chord + PAD_OUT + 0.01, "{len}");
                }
            } else {
                assert!(pads.is_empty(), "beat {b}");
            }
        }
    }

    /// Bells over `beats` beats of `m`.
    fn bells_in(m: Mood, beats: usize) -> usize {
        let mut c = Composer::default();
        (0..beats)
            .flat_map(|_| c.beat(m))
            .filter(|n| n.voice == Voice::Bell)
            .count()
    }

    #[test]
    fn home_rings_more_than_the_deep_and_a_fight_only_throbs() {
        let home = bells_in(mood(Place::Home, false), 400);
        let pit = bells_in(mood(Place::Pit, false), 400);
        let canyon = bells_in(mood(Place::Canyon, false), 400);
        assert!(home > 2 * pit && home > 2 * canyon, "{home} {pit} {canyon}");
        assert!(pit > 0 && canyon > 0, "the deep still has a motif");
        assert_eq!(bells_in(mood(Place::Home, true), 400), 0);
        let mut c = Composer::default();
        let fight: Vec<Note> = (0..16)
            .flat_map(|_| c.beat(mood(Place::Basin, true)))
            .collect();
        assert_eq!(fight.iter().filter(|n| n.voice == Voice::Pulse).count(), 16);
        assert!(
            fight
                .iter()
                .any(|n| n.voice == Voice::CalmDark && n.semis == TENSION)
        );
    }

    #[test]
    fn the_pit_is_hollow_and_low_and_the_canyon_cold_and_high() {
        let bells = |place: Place| {
            let mut c = Composer::default();
            let semis: Vec<f32> = (0..400)
                .flat_map(|_| c.beat(mood(place, false)))
                .filter(|n| n.voice == Voice::Bell)
                .map(|n| n.semis)
                .collect();
            semis.iter().sum::<f32>() / semis.len() as f32
        };
        assert!(bells(Place::Pit) < bells(Place::Home));
        assert!(bells(Place::Canyon) > bells(Place::Home));
        for place in [Place::Pit, Place::Canyon, Place::Lights] {
            let mut c = Composer::default();
            assert!(
                c.beat(mood(place, false))
                    .iter()
                    .all(|n| n.voice != Voice::CalmWarm)
            );
        }
        // Nothing lower than a phone's speaker can give a little of.
        for place in super::super::PLACES {
            for (bass, _) in progression(place) {
                assert!(
                    super::super::D3 * 2f32.powf(bass / 12.0) > 55.0,
                    "{place:?}"
                );
            }
        }
    }

    #[test]
    fn its_bells_keep_to_the_mode_its_stingers_use_and_rest_for_them() {
        for place in super::super::PLACES {
            let scale = mode(place);
            let mut c = Composer::default();
            for n in (0..400).flat_map(|_| c.beat(mood(place, false))) {
                if n.voice == Voice::Bell {
                    let pc = (n.semis.round() as i32).rem_euclid(12);
                    assert!(scale.contains(&pc), "{place:?}: {n:?}");
                }
            }
        }
        let mut c = Composer::default();
        c.quiet(400);
        assert_eq!(
            (0..400)
                .flat_map(|_| c.beat(mood(Place::Home, false)))
                .filter(|n| n.voice == Voice::Bell)
                .count(),
            0
        );
    }
}
