//! The "Themes" track: one piece built from a few short tunes (motifs) that
//! keep coming back, changed, wherever the crab goes (notes as letter
//! names; D3 is the home note, 147 Hz):
//!
//! - **The home theme**: A D E F, then E C A (the question) or E C D (the
//!   answer). A rising fourth and two steps up, then a turn back down.
//! - **The deep motif**: D A B♭ A. The home theme's leap turned upside down
//!   (a falling fourth) and a sighing step.
//! - **The octopus**: D E♭ C♯ E C D, low and slow: half-steps coiling out
//!   from D and back, like arms uncurling. Under its fight only.
//! - **The lights** (the sea pens on the canyon floor): A D E F♯ G♯ A, the
//!   home theme's opening carried on up, through the bright raised fourth,
//!   to the octave: the answer to the home theme's question.
//!
//! Each place has a mode on D, so the same tunes take its colour: home
//! Dorian (warm minor), the shallows a major pentatonic (bright), the basins
//! Aeolian (plain minor), the pit Phrygian (the dark half-step above D), the
//! canyon whole-tone (cold, floating) and the lights Lydian (wonder). Every
//! mode keeps D, so a change of place is smooth: it waits for the next chord
//! and starts the new place on its D chord.
//!
//! Music comes in periods of four chords, eight beats each: the lead asks
//! (the theme's question), a second voice answers or there's a rest, the
//! lead answers (the theme's answer), then a fragment or a rest. A seeded
//! generator picks instruments, small ornaments, fragments and rests, so it
//! never repeats exactly but stays recognisable.
//!
//! Layers follow what is happening: the pads and a low bass always; the
//! tunes while exploring; when a creature has noticed the crab, the tunes
//! thin out and the deep motif's falling fourth sounds low; in a fight the
//! tunes stop, a muffled heartbeat comes in every beat with an uneasy held
//! note, and the home theme's opening runs low and fast on the mallet; under
//! the octopus, its own coiling motif over sliding chords; with the shell
//! nearly broken, the home theme's leap rings softly, far off.

use super::{
    AEOLIAN, BEAT, CHORD_BEATS, DORIAN, Danger, LYDIAN, MAJOR_PENTATONIC, Mode, Mood, Note,
    PHRYGIAN, Place, Stinger, Voice, WHOLE_TONE, pitch,
};

/// Beats in a bar and a period (four chords: a question and an answer).
const BAR_BEATS: u64 = 4;
pub const PERIOD_BEATS: u64 = 32;
/// Pads: seconds swelling in, and fading out after the chord's eight beats
/// (so each chord overlaps the next one).
const PAD_IN: f32 = 3.0;
const PAD_OUT: f32 = 4.0;
/// The note held under a fight: a tritone above low D (A flat).
pub const TENSION: f32 = -6.0;

/// Each place's mode.
pub fn mode(place: Place) -> Mode {
    match place {
        Place::Home => DORIAN,
        Place::Shallows => MAJOR_PENTATONIC,
        Place::Basin => AEOLIAN,
        Place::Pit => PHRYGIAN,
        Place::Canyon => WHOLE_TONE,
        Place::Lights => LYDIAN,
    }
}

/// A tune: (beat it starts on, letter names above D3 (7 = D4), beats it
/// lasts).
pub type Motif = [(f32, i32, f32)];

/// The home theme asking: A D E F, E C A (ending open, on A).
pub const HOME_QUESTION: &Motif = &[
    (0.0, 4, 0.5),
    (0.5, 7, 1.5),
    (2.0, 8, 0.5),
    (2.5, 9, 1.5),
    (4.0, 8, 1.0),
    (5.0, 6, 1.0),
    (6.0, 4, 2.0),
];
/// The home theme answering: A D E F, E C D (home on D).
pub const HOME_ANSWER: &Motif = &[
    (0.0, 4, 0.5),
    (0.5, 7, 1.5),
    (2.0, 8, 0.5),
    (2.5, 9, 1.5),
    (4.0, 8, 1.0),
    (5.0, 6, 1.0),
    (6.0, 7, 2.0),
];
/// The theme's head: its first four notes, A D E F.
pub const HEAD: usize = 4;
/// The deep motif: D A B♭ A.
pub const DEEP: &Motif = &[(0.0, 7, 2.0), (2.0, 4, 1.0), (3.0, 5, 1.0), (4.0, 4, 4.0)];
/// The lights: A D E F♯ G♯ A (in Lydian), the head carried on to the octave.
pub const WONDER: &Motif = &[
    (0.0, 11, 0.5),
    (0.5, 14, 1.5),
    (2.0, 15, 0.5),
    (2.5, 16, 1.5),
    (4.0, 17, 1.0),
    (5.0, 18, 3.0),
];
/// The octopus: semitones from D3, in no mode (beat, semitones, beats).
pub const OCTOPUS: &[(f32, f32, f32)] = &[
    (0.0, 0.0, 1.0),
    (1.0, 1.0, 1.0),
    (2.0, -1.0, 1.0),
    (3.0, 2.0, 1.0),
    (4.0, -2.0, 2.0),
    (6.0, 0.0, 2.0),
];
/// The head's last step turned round, a beat late: F E, a sigh answering.
const SIGH: &Motif = &[(1.0, 9, 1.0), (2.0, 8, 2.0)];
/// The theme's leap alone (A up to D): a far-off reminder of home.
pub const LEAP: &Motif = &[(0.0, 4, 0.5), (0.5, 7, 2.0)];
/// The answer's last three notes, E C D: how home settles.
const SETTLE: &Motif = &[(1.0, 8, 1.0), (2.0, 6, 1.0), (3.0, 7, 3.0)];

/// A chord: its bass note and three above it (semitones from D3).
type Chord = (f32, [f32; 3]);

/// Each place's four chords, in turn: two asking, two answering. Every
/// note is in the place's mode.
fn progression(place: Place) -> [Chord; 4] {
    match place {
        // Dm9, G6, Cadd9, Asus4.
        Place::Home => [
            (-12.0, [3.0, 7.0, 14.0]),
            (-7.0, [2.0, 9.0, 12.0]),
            (-14.0, [2.0, 5.0, 12.0]),
            (-5.0, [0.0, 2.0, 7.0]),
        ],
        // D6/9, Bm7, Esus4, Asus4: only the pentatonic's notes.
        Place::Shallows => [
            (-12.0, [4.0, 9.0, 14.0]),
            (-15.0, [4.0, 7.0, 12.0]),
            (-10.0, [7.0, 9.0, 12.0]),
            (-5.0, [2.0, 7.0, 12.0]),
        ],
        // Dm9, B♭maj7, Gm7, Am7.
        Place::Basin => [
            (-12.0, [3.0, 7.0, 14.0]),
            (-16.0, [3.0, 7.0, 12.0]),
            (-7.0, [3.0, 8.0, 12.0]),
            (-5.0, [2.0, 5.0, 10.0]),
        ],
        // Bare D, E♭maj7, Cm9, D with the half-step above.
        Place::Pit => [
            (-12.0, [-5.0, 0.0, 7.0]),
            (-11.0, [5.0, 8.0, 12.0]),
            (-14.0, [1.0, 5.0, 12.0]),
            (-12.0, [1.0, 7.0, 12.0]),
        ],
        // Augmented chords on D, C, B♭ and E over D.
        Place::Canyon => [
            (-12.0, [4.0, 8.0, 14.0]),
            (-14.0, [2.0, 6.0, 12.0]),
            (-16.0, [0.0, 4.0, 10.0]),
            (-12.0, [2.0, 6.0, 10.0]),
        ],
        // Dmaj9, E over D, Bm9, Dmaj7♯11.
        Place::Lights => [
            (-12.0, [4.0, 11.0, 14.0]),
            (-12.0, [2.0, 6.0, 9.0]),
            (-15.0, [4.0, 7.0, 11.0]),
            (-12.0, [4.0, 6.0, 11.0]),
        ],
    }
}

/// Under the octopus: bare fifths sliding a half-step up and down, coiling
/// like its motif.
const OCTOPUS_CHORDS: [Chord; 4] = [
    (-12.0, [-5.0, 0.0, 7.0]),
    (-11.0, [-4.0, 1.0, 8.0]),
    (-12.0, [-5.0, 0.0, 7.0]),
    (-13.0, [-6.0, -1.0, 6.0]),
];

/// How a tune is played: on which voice, moved how many letter names, its
/// beats stretched (2 = twice as slow), upside down about its first note,
/// how loud, through the long echo, starting how many beats in.
#[derive(Clone, Copy)]
struct How {
    voice: Voice,
    shift: i32,
    stretch: f32,
    invert: bool,
    gain: f32,
    echo: bool,
    at: f32,
}

impl How {
    fn on(voice: Voice) -> How {
        How {
            voice,
            shift: 0,
            stretch: 1.0,
            invert: false,
            gain: voice.gain(),
            echo: false,
            at: 0.0,
        }
    }
    fn shift(self, shift: i32) -> How {
        How { shift, ..self }
    }
    fn stretch(self, stretch: f32) -> How {
        How { stretch, ..self }
    }
    fn invert(self, invert: bool) -> How {
        How { invert, ..self }
    }
    fn gain(self, gain: f32) -> How {
        How { gain, ..self }
    }
    fn echo(self) -> How {
        How { echo: true, ..self }
    }
    fn at(self, at: f32) -> How {
        How { at, ..self }
    }
}

/// A note on a tune voice: a flute swells in and fades over its length (a
/// little legato), a mallet or a bell rings out.
fn tune_note(voice: Voice, semis: f32, secs: f32, gain: f32, echo: bool) -> Note {
    let (rise, hold, fall) = if voice.looped() {
        let rise = (0.3 * secs).min(0.12);
        (rise, (secs - rise).max(0.05), 0.45)
    } else {
        (0.0, 0.0, 0.0)
    };
    Note {
        voice,
        at: 0.0,
        semis,
        gain,
        rise,
        hold,
        fall,
        echo,
    }
}

/// `motif` in `mode`, played `how`: (beats from the start, note).
fn tune(mode: &Mode, motif: &Motif, how: How) -> Vec<(f32, Note)> {
    let axis = motif.first().map_or(0, |m| m.1);
    motif
        .iter()
        .map(|&(when, step, beats)| {
            let step = if how.invert { 2 * axis - step } else { step } + how.shift;
            let secs = beats * how.stretch * BEAT;
            let note = tune_note(how.voice, pitch(mode, step), secs, how.gain, how.echo);
            (how.at + when * how.stretch, note)
        })
        .collect()
}

/// A held note on a looped voice.
fn held(voice: Voice, semis: f32, gain: f32, rise: f32, hold: f32, fall: f32) -> Note {
    Note {
        voice,
        at: 0.0,
        semis,
        gain,
        rise,
        hold,
        fall,
        echo: false,
    }
}

/// Notes waiting for their beat: (beat, note with `at` inside the beat).
type Queue = Vec<(u64, Note)>;

fn queue(q: &mut Queue, from: u64, notes: Vec<(f32, Note)>) {
    for (beats, mut note) in notes {
        let whole = beats.floor();
        note.at = (beats - whole) * BEAT;
        q.push((from + whole as u64, note));
    }
}

/// Take `q`'s notes for `beat` (and drop any it missed).
fn due(q: &mut Queue, beat: u64) -> Vec<Note> {
    let mut now = Vec::new();
    q.retain(|(b, n)| {
        if *b == beat {
            now.push(*n);
        }
        *b > beat
    });
    now
}

/// Writes the music, a beat at a time.
pub struct Composer {
    beat: u64,
    rng: u32,
    /// The place playing (a new one starts at the next chord).
    place: Place,
    started: bool,
    /// Beats into the current period.
    pos: u64,
    /// The voice that asked this period's question.
    lead: Voice,
    /// The tunes waiting to play, and the danger and shell layers'.
    melody: Queue,
    layer: Queue,
    /// No new tune notes before this beat (a stinger is playing).
    quiet_until: u64,
}

impl Default for Composer {
    fn default() -> Self {
        Self::new(0x2545_F491)
    }
}

impl Composer {
    pub fn new(seed: u32) -> Self {
        Self {
            beat: 0,
            rng: seed.max(1),
            place: Place::Home,
            started: false,
            pos: 0,
            lead: Voice::Flute,
            melody: Vec::new(),
            layer: Vec::new(),
            quiet_until: 0,
        }
    }

    /// 0 up to 1.
    fn roll(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    fn chance(&mut self, p: f32) -> bool {
        self.roll() < p
    }

    /// No new tune notes for the next `beats` beats.
    pub fn quiet(&mut self, beats: u64) {
        self.quiet_until = self.beat + beats;
    }

    /// Start afresh at the next beat: a new period on the place's D chord
    /// (when this track comes in).
    pub fn restart(&mut self) {
        self.started = false;
        self.pos = 0;
        self.melody.clear();
        self.layer.clear();
    }

    /// The next beat's notes, for `mood`.
    pub fn beat(&mut self, mood: Mood) -> Vec<Note> {
        let beat = self.beat;
        self.beat += 1;
        let mut notes = Vec::new();
        if self.pos.is_multiple_of(CHORD_BEATS) {
            // A new place starts at a chord, on its D chord, a new period.
            if !self.started || mood.place != self.place {
                self.started = true;
                self.place = mood.place;
                self.pos = 0;
                self.melody.clear();
            }
            let slot = (self.pos / CHORD_BEATS) as usize;
            self.chord(slot, mood, &mut notes);
            let m = mode(self.place);
            match mood.danger {
                Danger::Calm | Danger::Aware => {
                    let aware = mood.danger == Danger::Aware;
                    if self.melody.is_empty() && beat >= self.quiet_until {
                        let phrase = self.plan(slot, aware);
                        queue(&mut self.melody, beat, phrase);
                    }
                    if aware {
                        // The deep motif's falling fourth, low: an omen.
                        notes.push(held(Voice::DarkPad, pitch(&m, 0), 0.045, 1.5, 1.0, 1.5));
                        let low = held(Voice::DarkPad, pitch(&m, -3), 0.045, 1.0, 2.0, 2.0);
                        queue(&mut self.layer, beat, vec![(3.0, low)]);
                    }
                }
                Danger::Fight => self.melody.clear(),
                Danger::Octopus => {
                    self.melody.clear();
                    let coil = OCTOPUS
                        .iter()
                        .flat_map(|&(when, semis, beats)| {
                            let secs = beats * BEAT;
                            [
                                (
                                    when,
                                    held(Voice::DarkPad, semis, 0.06, 0.2, secs - 0.2, 0.6),
                                ),
                                (
                                    when,
                                    tune_note(Voice::Mallet, semis + 12.0, secs, 0.05, false),
                                ),
                            ]
                        })
                        .collect();
                    queue(&mut self.layer, beat, coil);
                }
            }
            if mood.low_shell {
                let far = How::on(Voice::Glass).shift(7).gain(0.04).echo().at(4.0);
                queue(&mut self.layer, beat, tune(&m, LEAP, far));
            }
        }
        if mood.danger >= Danger::Fight {
            notes.push(Note {
                voice: Voice::Pulse,
                at: 0.0,
                semis: 0.0,
                gain: 0.3,
                rise: 0.0,
                hold: 0.0,
                fall: 0.0,
                echo: false,
            });
            if mood.danger == Danger::Fight && self.pos.is_multiple_of(BAR_BEATS) {
                // The home theme's head, twice as fast on the low mallet;
                // every other bar upside down and higher: call, response.
                let upside_down = (self.pos / BAR_BEATS) % 2 == 1;
                let how = How::on(Voice::Mallet)
                    .shift(if upside_down { 0 } else { -7 })
                    .stretch(0.5)
                    .invert(upside_down)
                    .gain(0.1);
                let m = mode(self.place);
                queue(&mut self.layer, beat, tune(&m, &HOME_QUESTION[..HEAD], how));
            }
        }
        let tunes = due(&mut self.melody, beat);
        if mood.danger <= Danger::Aware && beat >= self.quiet_until {
            notes.extend(tunes);
        }
        notes.extend(due(&mut self.layer, beat));
        self.pos = (self.pos + 1) % PERIOD_BEATS;
        notes
    }

    /// The pads and the bass for chord `slot` of the period.
    fn chord(&self, slot: usize, mood: Mood, notes: &mut Vec<Note>) {
        let (bass, upper) = if mood.danger == Danger::Octopus {
            OCTOPUS_CHORDS[slot]
        } else {
            progression(self.place)[slot]
        };
        let voice =
            if matches!(self.place, Place::Pit | Place::Canyon) || mood.danger >= Danger::Fight {
                Voice::DarkPad
            } else {
                Voice::WarmPad
            };
        let span = CHORD_BEATS as f32 * BEAT;
        let pad = |voice, semis, gain| held(voice, semis, gain, PAD_IN, span - PAD_IN, PAD_OUT);
        notes.push(pad(Voice::Sub, bass, 0.08));
        notes.push(pad(voice, bass + 12.0, 0.04));
        for u in upper {
            notes.push(pad(voice, u, 0.045));
        }
        if mood.danger == Danger::Fight {
            notes.push(pad(Voice::DarkPad, TENSION, 0.045));
        }
    }

    /// The home theme, asking or answering, now and then with a turn (G)
    /// slipped in before its F.
    fn theme(&mut self, answer: bool) -> Vec<(f32, i32, f32)> {
        let mut t = if answer { HOME_ANSWER } else { HOME_QUESTION }.to_vec();
        if self.chance(0.2) {
            t[2].2 = 0.25;
            t.insert(3, (2.25, 10, 0.25));
        }
        t
    }

    /// The tune for chord `slot` of a period (beats from its start).
    fn plan(&mut self, slot: usize, aware: bool) -> Vec<(f32, Note)> {
        use Voice::{Flute, Glass, Mallet};
        // Wary: only the lead's question and answer, quieter (down in the
        // dark, only the question).
        let sparse = matches!(self.place, Place::Pit | Place::Canyon);
        if aware && (slot % 2 == 1 || (sparse && slot == 2)) {
            return Vec::new();
        }
        let m = mode(self.place);
        let other = |v: Voice| if v == Flute { Mallet } else { Flute };
        let mut phrase = match (self.place, slot) {
            (Place::Home, 0) => {
                if self.chance(0.1) {
                    // A rest that matters: the theme waits a period.
                    Vec::new()
                } else {
                    self.lead = if self.chance(0.75) { Flute } else { Mallet };
                    tune(&m, &self.theme(false), How::on(self.lead))
                }
            }
            (Place::Home, 1) => match self.roll() {
                r if r < 0.45 => tune(&m, SIGH, How::on(Glass).shift(7).echo()),
                r if r < 0.7 => tune(&m, &DEEP[..2], How::on(Mallet).shift(-7).at(2.0)),
                _ => Vec::new(),
            },
            (Place::Home, 2) => {
                let v = if self.chance(0.3) {
                    other(self.lead)
                } else {
                    self.lead
                };
                tune(&m, &self.theme(true), How::on(v))
            }
            (Place::Home, _) => match self.roll() {
                r if r < 0.35 => tune(&m, DEEP, How::on(Mallet).shift(-7)),
                r if r < 0.65 => tune(&m, LEAP, How::on(Glass).shift(7).echo().at(2.0)),
                _ => Vec::new(),
            },
            (Place::Shallows, 0) => tune(&m, &self.theme(false), How::on(Mallet).shift(7)),
            (Place::Shallows, 1) => {
                if self.chance(0.6) {
                    tune(&m, SETTLE, How::on(Flute).shift(7))
                } else {
                    Vec::new()
                }
            }
            (Place::Shallows, 2) => {
                if self.chance(0.3) {
                    tune(&m, &self.theme(true), How::on(Flute).shift(7))
                } else {
                    tune(&m, &self.theme(true), How::on(Mallet).shift(7))
                }
            }
            (Place::Shallows, _) => {
                if self.chance(0.5) {
                    tune(&m, LEAP, How::on(Glass).shift(7).echo().at(1.0))
                } else {
                    Vec::new()
                }
            }
            (Place::Basin, 0) => tune(&m, DEEP, How::on(Flute)),
            (Place::Basin, 1) => {
                if self.chance(0.4) {
                    tune(&m, LEAP, How::on(Glass).shift(7).echo().at(2.0))
                } else {
                    Vec::new()
                }
            }
            (Place::Basin, 2) => tune(&m, &HOME_QUESTION[..HEAD], How::on(Mallet).stretch(2.0)),
            (Place::Basin, _) => {
                if self.chance(0.5) {
                    tune(&m, &DEEP[..2], How::on(Mallet).shift(-7).at(1.0))
                } else {
                    Vec::new()
                }
            }
            // The deep motif twice as slow, low on the flute: sixteen beats.
            (Place::Pit, 0) => tune(&m, DEEP, How::on(Flute).shift(-7).stretch(2.0)),
            (Place::Pit, 2) => {
                if self.chance(0.6) {
                    tune(&m, &HOME_QUESTION[..HEAD], How::on(Mallet).shift(-7))
                } else {
                    tune(&m, LEAP, How::on(Glass).shift(7).gain(0.045).echo().at(1.0))
                }
            }
            (Place::Pit, _) => Vec::new(),
            (Place::Canyon, 0) => tune(&m, &self.theme(false), How::on(Glass).shift(7).echo()),
            (Place::Canyon, 2) => tune(&m, DEEP, How::on(Flute).invert(true).echo()),
            (Place::Canyon, 3) => {
                if self.chance(0.4) {
                    tune(&m, SIGH, How::on(Glass).shift(7).echo())
                } else {
                    Vec::new()
                }
            }
            (Place::Canyon, _) => Vec::new(),
            (Place::Lights, 0) => tune(&m, &self.theme(false), How::on(Flute)),
            // The lights answer the home theme's question.
            (Place::Lights, 1) => tune(&m, WONDER, How::on(Glass).echo()),
            (Place::Lights, 2) => {
                let v = if self.chance(0.5) { Flute } else { Mallet };
                tune(&m, &self.theme(true), How::on(v))
            }
            (Place::Lights, _) => {
                if self.chance(0.5) {
                    tune(&m, WONDER, How::on(Mallet).shift(-7))
                } else {
                    tune(&m, &WONDER[4..], How::on(Flute).shift(-7).at(2.0))
                }
            }
        };
        if aware {
            for (_, n) in &mut phrase {
                n.gain *= 0.8;
            }
        }
        phrase
    }
}

/// `s`'s notes in `m` (the mode playing, on either track), `at` seconds
/// after it starts.
pub fn stinger(s: Stinger, m: &Mode) -> Vec<Note> {
    use Voice::{DarkPad, Flute, Glass, Mallet, WarmPad};
    let notes: Vec<(f32, Note)> = match s {
        Stinger::Egg => tune(
            m,
            &[(0.0, 7, 0.2), (0.11, 9, 0.2), (0.22, 11, 0.4)],
            How::on(Mallet).shift(7).gain(0.07),
        ),
        Stinger::LevelUp => tune(
            m,
            &HOME_QUESTION[..HEAD],
            How::on(Glass).shift(7).stretch(0.28).echo(),
        ),
        Stinger::FeatReady => {
            let mut n = tune(m, WONDER, How::on(Glass).stretch(0.25).echo());
            n.push((1.25, tune_note(Mallet, pitch(m, 11), 1.0, 0.08, false)));
            n
        }
        Stinger::Hardened => {
            let mut n = tune(m, SETTLE, How::on(Flute).stretch(0.45).at(-0.45));
            n.push((0.9, tune_note(Glass, pitch(m, 14), 1.0, 0.05, true)));
            n
        }
        Stinger::Found => tune(m, DEEP, How::on(Glass).shift(7).stretch(0.45).echo()),
        Stinger::Lights => {
            let mut n = tune(m, WONDER, How::on(Glass).stretch(0.6).echo());
            n.push((3.0, tune_note(Flute, pitch(m, 11), 2.5, 0.06, true)));
            n
        }
        Stinger::Beaten => {
            let mut n = tune(
                m,
                &[(0.0, 9, 0.25), (0.22, 8, 0.25), (0.44, 7, 1.0)],
                How::on(Mallet).gain(0.1),
            );
            n.push((0.44, held(DarkPad, pitch(m, -7), 0.06, 0.3, 1.0, 2.0)));
            n
        }
        Stinger::Death => {
            let mut n = tune(
                m,
                &[(0.0, 7, 0.6), (0.6, 4, 0.6), (1.2, 2, 0.7), (1.9, 0, 2.5)],
                How::on(Flute).gain(0.07),
            );
            n.push((0.0, held(DarkPad, pitch(m, -7), 0.05, 1.0, 1.5, 2.5)));
            n
        }
        Stinger::Ending => {
            let mut n = tune(m, HOME_ANSWER, How::on(Flute).stretch(0.7));
            n.push((4.2, tune_note(Glass, pitch(m, 14), 1.0, 0.06, true)));
            for step in [0, 2, 4] {
                n.push((4.2, held(WarmPad, pitch(m, step), 0.035, 1.0, 1.0, 2.5)));
            }
            n
        }
    };
    notes
        .into_iter()
        .map(|(beats, mut n)| {
            n.at = beats.max(0.0) * BEAT;
            n
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::{D3, PLACES};
    use super::*;

    fn mood(place: Place, danger: Danger) -> Mood {
        Mood {
            place,
            danger,
            low_shell: false,
        }
    }

    fn calm(place: Place) -> Mood {
        mood(place, Danger::Calm)
    }

    /// `beats` beats of `m` from a fresh composer (seeded `seed`), each
    /// note with when it starts.
    fn write(m: Mood, beats: u64, seed: u32) -> Vec<(f64, Note)> {
        let mut c = Composer::new(seed);
        (0..beats)
            .flat_map(|b| {
                c.beat(m)
                    .into_iter()
                    .map(move |n| (b as f64 + f64::from(n.at), n))
            })
            .collect()
    }

    /// The pitches `voice` plays, in order.
    fn line(notes: &[(f64, Note)], voice: Voice) -> Vec<f32> {
        let mut on: Vec<&(f64, Note)> = notes.iter().filter(|n| n.1.voice == voice).collect();
        on.sort_by(|a, b| a.0.total_cmp(&b.0));
        on.iter().map(|n| n.1.semis).collect()
    }

    /// Does `line` play `tune`, in any octave?
    fn plays(line: &[f32], tune: &[f32]) -> bool {
        line.windows(tune.len()).any(|w| {
            let shift = w[0] - tune[0];
            shift % 12.0 == 0.0 && w.iter().zip(tune).all(|(a, b)| a - b == shift)
        })
    }

    /// `motif`'s pitches in `place`'s mode, upside down if `invert`.
    fn notes_of(motif: &Motif, place: Place, invert: bool) -> Vec<f32> {
        let m = mode(place);
        let axis = motif[0].1;
        motif
            .iter()
            .map(|&(_, s, _)| pitch(&m, if invert { 2 * axis - s } else { s }))
            .collect()
    }

    /// Does some tune voice in `notes` play `tune`?
    fn heard(notes: &[(f64, Note)], tune: &[f32]) -> bool {
        [Voice::Flute, Voice::Mallet, Voice::Glass]
            .iter()
            .any(|&v| plays(&line(notes, v), tune))
    }

    fn pitch_class(semis: f32) -> i32 {
        (semis.round() as i32).rem_euclid(12)
    }

    #[test]
    fn a_seed_always_plays_the_same_music_and_another_varies_it() {
        let walk = [
            calm(Place::Home),
            mood(Place::Basin, Danger::Aware),
            mood(Place::Pit, Danger::Fight),
            calm(Place::Lights),
        ];
        let write = |seed| {
            let mut c = Composer::new(seed);
            (0..256)
                .flat_map(|b| c.beat(walk[b / 64]))
                .collect::<Vec<_>>()
        };
        assert_eq!(write(7), write(7));
        assert_ne!(write(7), write(8));
        // Varied, but only by ornaments, instruments, fragments and rests:
        // the theme is there under every seed.
        for seed in 1..20 {
            let notes = write(seed)
                .into_iter()
                .enumerate()
                .map(|(i, n)| (i as f64, n))
                .collect::<Vec<_>>();
            let head = notes_of(&HOME_QUESTION[..HEAD], Place::Home, false);
            assert!(heard(&notes, &head), "seed {seed}");
        }
    }

    #[test]
    fn the_home_theme_threads_through_every_place() {
        for place in PLACES {
            let notes = write(calm(place), 12 * PERIOD_BEATS, 3);
            let head = notes_of(&HOME_QUESTION[..HEAD], place, false);
            assert!(heard(&notes, &head), "{place:?}: no home theme {head:?}");
            let deep = notes_of(DEEP, place, false);
            let deep_inverted = notes_of(DEEP, place, true);
            let has_deep = heard(&notes, &deep) || heard(&notes, &deep_inverted);
            let wants_deep = matches!(
                place,
                Place::Home | Place::Basin | Place::Pit | Place::Canyon
            );
            assert!(!wants_deep || has_deep, "{place:?}: no deep motif");
            let wonder = notes_of(WONDER, place, false);
            assert_eq!(heard(&notes, &wonder), place == Place::Lights, "{place:?}");
        }
        // The whole theme, question and answer, at home.
        let notes = write(calm(Place::Home), 12 * PERIOD_BEATS, 3);
        for theme in [HOME_QUESTION, HOME_ANSWER] {
            assert!(heard(&notes, &notes_of(theme, Place::Home, false)));
        }
    }

    #[test]
    fn each_place_keeps_to_its_mode() {
        for place in PLACES {
            let scale = mode(place);
            for danger in [Danger::Calm, Danger::Aware] {
                for low_shell in [false, true] {
                    let m = Mood {
                        place,
                        danger,
                        low_shell,
                    };
                    for (_, n) in write(m, 4 * PERIOD_BEATS, 11) {
                        assert!(
                            scale.contains(&pitch_class(n.semis)),
                            "{place:?} {danger:?}: {n:?} is out of its mode"
                        );
                    }
                }
            }
        }
        assert_eq!(mode(Place::Home)[5], 9, "home's warm sixth (Dorian)");
        assert!(!mode(Place::Shallows).contains(&3), "no minor third");
        assert_eq!(mode(Place::Pit)[1], 1, "the pit's half-step above D");
        assert!(!mode(Place::Canyon).contains(&7), "no fifth in the canyon");
        assert_eq!(mode(Place::Lights)[3], 6, "the lights' raised fourth");
    }

    #[test]
    fn a_new_place_waits_for_the_chord_and_starts_on_d() {
        let mut c = Composer::default();
        for _ in 0..3 {
            c.beat(calm(Place::Home));
        }
        // Mid-chord: still home's notes.
        let mid = c.beat(calm(Place::Pit));
        assert!(mid.iter().all(|n| n.voice != Voice::DarkPad));
        for _ in 4..8 {
            c.beat(calm(Place::Pit));
        }
        let chord = c.beat(calm(Place::Pit));
        let sub = chord.iter().find(|n| n.voice == Voice::Sub).unwrap();
        assert_eq!(pitch_class(sub.semis), 0, "the pit starts on its D chord");
        assert!(chord.iter().any(|n| n.voice == Voice::DarkPad));
    }

    #[test]
    fn layers_follow_the_danger() {
        let count = |m: Mood, f: &dyn Fn(&Note) -> bool| {
            write(m, 2 * PERIOD_BEATS, 5)
                .iter()
                .filter(|n| f(&n.1))
                .count()
        };
        let tunes = |n: &Note| n.voice.melodic();
        let pulse = |n: &Note| n.voice == Voice::Pulse;
        let bass = |n: &Note| n.voice == Voice::Sub;
        for place in PLACES {
            let calm_tunes = count(calm(place), &tunes);
            let wary_tunes = count(mood(place, Danger::Aware), &tunes);
            assert!(
                wary_tunes < calm_tunes,
                "{place:?}: {wary_tunes} {calm_tunes}"
            );
            // The base bed is always there.
            for danger in [Danger::Calm, Danger::Aware, Danger::Fight, Danger::Octopus] {
                assert_eq!(count(mood(place, danger), &bass), 8, "{place:?} {danger:?}");
            }
            // The heartbeat only in fights, every beat.
            assert_eq!(count(calm(place), &pulse), 0);
            assert_eq!(count(mood(place, Danger::Aware), &pulse), 0);
            assert_eq!(count(mood(place, Danger::Fight), &pulse), 64);
            assert_eq!(count(mood(place, Danger::Octopus), &pulse), 64);
        }
        // A fight: the uneasy note, and the home theme's head on the mallet,
        // fast and low; no flute.
        let fight = write(mood(Place::Basin, Danger::Fight), PERIOD_BEATS, 5);
        assert!(fight.iter().any(|n| n.1.semis == TENSION));
        let head = notes_of(&HOME_QUESTION[..HEAD], Place::Basin, false);
        assert!(plays(&line(&fight, Voice::Mallet), &head));
        assert!(fight.iter().all(|n| n.1.voice != Voice::Flute));
        // The octopus coils.
        let octopus = write(mood(Place::Basin, Danger::Octopus), PERIOD_BEATS, 5);
        let coil: Vec<f32> = OCTOPUS.iter().map(|o| o.1).collect();
        assert!(plays(&line(&octopus, Voice::DarkPad), &coil));
        // A nearly broken shell: the theme's leap, far off.
        let low = write(
            Mood {
                low_shell: true,
                ..mood(Place::Basin, Danger::Fight)
            },
            PERIOD_BEATS,
            5,
        );
        let leap = notes_of(LEAP, Place::Basin, false);
        assert!(plays(&line(&low, Voice::Glass), &leap));
    }

    #[test]
    fn the_bass_stays_above_55_hz() {
        for place in PLACES {
            for danger in [Danger::Calm, Danger::Fight, Danger::Octopus] {
                for (_, n) in write(mood(place, danger), PERIOD_BEATS, 9) {
                    if n.voice != Voice::Pulse {
                        let hz = D3 * 2f32.powf(n.semis / 12.0);
                        assert!(hz > 55.0, "{n:?} at {hz} Hz");
                    }
                }
            }
        }
    }
}
