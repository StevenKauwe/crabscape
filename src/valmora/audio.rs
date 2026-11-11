//! Dragon's original tiny synthesizer. Playback only: no microphone or save access.
//! Recipes follow web/dragon/index.html; PCM oscillators approximate WebAudio's
//! band-limited implementation, so this is parameter parity, not bit-identical audio.
mod music;
#[cfg(any(test, not(target_arch = "wasm32")))]
mod pcm;
#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(not(target_arch = "wasm32"))]
mod worker;
pub use music::{Music, MusicScene};
#[cfg(any(test, not(target_arch = "wasm32")))]
use pcm::wav;
#[cfg(target_arch = "wasm32")]
pub use web::reap as reap_browser_audio;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wave {
    Sine,
    Triangle,
    Square,
    Sawtooth,
    Lowpass,
    Highpass,
    Bandpass,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub frequency: f64,
    pub duration: f64,
    pub wave: Wave,
    pub volume: f64,
    pub end_frequency: Option<f64>,
    pub delay: f64,
    pub music: bool,
}
fn tone(
    frequency: f64,
    duration: f64,
    wave: Wave,
    volume: f64,
    end_frequency: Option<f64>,
    delay: f64,
) -> Note {
    Note {
        frequency,
        duration,
        wave,
        volume,
        end_frequency,
        delay,
        music: false,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Gem(usize),
    Gold,
    Breath(Breath),
    Weapon(Path),
    Thud,
    Poof,
    Grr,
    Throw,
    Zap,
    Bonk,
    Hatch,
    Grow,
    Wave,
    Win,
    Pick,
    Spell,
    Shadow,
    DadBoom,
    MegaLaunch,
    MegaBoom,
    Rimshot,
    Smash,
    Wiggle,
    Correct,
    Wrong,
    Freeze,
    Thaw,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breath {
    Spark,
    Boulder,
    Frost,
    Star,
    Fire,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Shadow,
    Storm,
    Guardian,
    Other,
}
#[derive(Default)]
pub struct Synth {
    spark_tick: u64,
}
impl Synth {
    pub fn recipe(&mut self, effect: Effect) -> Vec<Note> {
        use Wave::*;
        let t = tone;
        let n = |d, v, f, w, delay| t(f, d, w, v, None, delay);
        let chord = |fs: &[f64], d, w, v, step, offset| {
            fs.iter()
                .enumerate()
                .map(|(i, &f)| t(f, d, w, v, None, offset + i as f64 * step))
                .collect::<Vec<_>>()
        };
        match effect {
            Effect::Gem(i) => {
                let scale = [0., 2., 4., 7., 9., 12., 14., 16., 19., 21., 24.];
                let f = 523.25 * 2_f64.powf(scale[i.min(10)] / 12.);
                vec![
                    t(f, 0.14, Sine, 0.18, None, 0.),
                    t(f * 1.5, 0.2, Triangle, 0.09, None, 0.05),
                ]
            }
            Effect::Gold => chord(&[784., 988., 1175., 1568.], 0.18, Triangle, 0.13, 0.05, 0.),
            Effect::Breath(Breath::Spark) => {
                self.spark_tick = self.spark_tick.wrapping_add(1);
                if self.spark_tick % 2 == 1 {
                    vec![t(1300., 0.04, Square, 0.025, Some(900.), 0.)]
                } else {
                    vec![]
                }
            }
            Effect::Breath(Breath::Boulder) => vec![
                n(0.3, 0.25, 300., Lowpass, 0.),
                t(110., 0.25, Sine, 0.12, Some(60.), 0.),
            ],
            Effect::Breath(Breath::Frost) => vec![n(0.2, 0.14, 4000., Highpass, 0.)],
            Effect::Breath(Breath::Star) => vec![t(1568., 0.08, Triangle, 0.05, Some(1976.), 0.)],
            Effect::Breath(Breath::Fire) => vec![
                n(0.2, 0.18, 1100., Bandpass, 0.),
                t(200., 0.16, Sawtooth, 0.035, Some(90.), 0.),
            ],
            Effect::Weapon(Path::Shadow) => vec![n(0.08, 0.12, 2500., Bandpass, 0.)],
            Effect::Weapon(Path::Storm) => vec![n(0.08, 0.12, 3600., Bandpass, 0.)],
            Effect::Weapon(Path::Guardian) => vec![t(620., 0.12, Triangle, 0.08, Some(880.), 0.)],
            Effect::Weapon(Path::Other) => vec![t(240., 0.12, Triangle, 0.08, Some(180.), 0.)],
            Effect::Thud => vec![t(220., 0.1, Square, 0.05, Some(120.), 0.)],
            Effect::Poof => vec![
                n(0.35, 0.26, 600., Lowpass, 0.),
                t(500., 0.28, Sine, 0.11, Some(1100.), 0.),
            ],
            Effect::Grr => vec![
                t(140., 0.25, Sawtooth, 0.07, Some(90.), 0.),
                n(0.25, 0.12, 500., Lowpass, 0.),
            ],
            Effect::Throw => vec![n(0.15, 0.1, 900., Bandpass, 0.)],
            Effect::Zap => vec![
                n(0.3, 0.3, 2500., Highpass, 0.),
                t(900., 0.2, Square, 0.06, Some(200.), 0.),
            ],
            Effect::Bonk => vec![
                t(320., 0.1, Square, 0.07, Some(160.), 0.),
                t(160., 0.3, Sine, 0.15, Some(460.), 0.08),
            ],
            Effect::Hatch => chord(
                &[523., 659., 784., 1047., 1319., 1568.],
                0.32,
                Triangle,
                0.13,
                0.09,
                0.,
            ),
            Effect::Grow => chord(
                &[392., 523., 659., 784., 1047.],
                0.28,
                Triangle,
                0.14,
                0.07,
                0.,
            ),
            Effect::Wave => chord(&[523., 784.], 0.25, Triangle, 0.13, 0.12, 0.),
            Effect::Win => chord(
                &[523., 659., 784., 1047., 784., 1047., 1319.],
                0.3,
                Triangle,
                0.14,
                0.11,
                0.,
            ),
            Effect::Pick => vec![t(660., 0.1, Triangle, 0.12, None, 0.)],
            Effect::Spell => {
                let mut notes = chord(&[880., 1175., 1568.], 0.3, Sine, 0.1, 0.06, 0.);
                notes.push(n(0.4, 0.06, 5000., Highpass, 0.));
                notes
            }
            Effect::Shadow => vec![
                t(300., 0.4, Sine, 0.12, Some(120.), 0.),
                n(0.4, 0.1, 400., Lowpass, 0.),
            ],
            Effect::DadBoom => {
                let mut notes = vec![
                    t(55., 0.9, Sawtooth, 0.25, Some(30.), 0.),
                    n(0.8, 0.4, 300., Lowpass, 0.),
                ];
                notes.extend(chord(
                    &[262., 330., 392., 523.],
                    0.7,
                    Triangle,
                    0.1,
                    0.05,
                    0.25,
                ));
                notes
            }
            Effect::MegaLaunch => vec![
                t(200., 0.6, Sawtooth, 0.12, Some(900.), 0.),
                n(0.6, 0.15, 1200., Bandpass, 0.),
            ],
            Effect::MegaBoom => vec![
                n(1.6, 0.6, 180., Lowpass, 0.),
                t(70., 1.2, Sine, 0.4, Some(28.), 0.),
                t(140., 0.8, Square, 0.08, Some(50.), 0.05),
                n(0.9, 0.2, 2500., Highpass, 0.1),
            ],
            Effect::Rimshot => vec![
                t(180., 0.12, Sine, 0.35, Some(120.), 0.),
                t(140., 0.12, Sine, 0.35, Some(100.), 0.16),
                n(0.5, 0.18, 7000., Highpass, 0.34),
            ],
            Effect::Smash => vec![
                n(0.6, 0.45, 220., Lowpass, 0.),
                t(90., 0.5, Sine, 0.3, Some(40.), 0.),
                t(600., 0.15, Square, 0.05, Some(300.), 0.02),
            ],
            Effect::Wiggle => [0., 0.16, 0.32]
                .into_iter()
                .map(|d| t(392., 0.12, Triangle, 0.12, Some(330.), d))
                .collect(),
            Effect::Correct => chord(&[659., 784., 1047.], 0.2, Triangle, 0.16, 0.08, 0.),
            Effect::Wrong => vec![
                t(330., 0.25, Sine, 0.15, Some(294.), 0.),
                t(294., 0.4, Sine, 0.15, Some(220.), 0.22),
            ],
            Effect::Freeze => {
                let mut notes = vec![n(0.5, 0.2, 3000., Highpass, 0.)];
                notes.extend(chord(&[1568., 1319., 1175.], 0.25, Sine, 0.08, 0.07, 0.));
                notes
            }
            Effect::Thaw => chord(&[523., 784., 1047.], 0.22, Sine, 0.12, 0.1, 0.),
        }
    }
}

/// Playback resources are scoped to Valmora. Caller stops them when leaving or muting.
#[derive(bevy::prelude::Component)]
pub struct ValmoraSound;
#[derive(bevy::prelude::Component)]
pub struct ValmoraMusic;
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct SpokenPiece {
    pub text: String,
    pub quoted: bool,
    pub rate: f32,
    pub pitch: f32,
}
/// Source narrPieces sentence/quotation boundaries and narration parameters.
/// Native speech synthesis is not provided by Bevy; UI must not gate Continue on it.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn narration_pieces(text: &str) -> Vec<SpokenPiece> {
    let mut out = Vec::new();
    let mut chunk = String::new();
    let mut quoted = false;
    fn emit(chunk: &mut String, quoted: bool, out: &mut Vec<SpokenPiece>) {
        let text = chunk.trim();
        if !text.is_empty() {
            let bang = text.contains('!');
            out.push(SpokenPiece {
                text: text.into(),
                quoted,
                rate: if quoted {
                    0.98
                } else if bang {
                    1.04
                } else {
                    1.
                },
                pitch: if quoted {
                    0.92
                } else if bang {
                    1.1
                } else {
                    1.02
                },
            });
        }
        chunk.clear();
    }
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            if quoted {
                chunk.push(c);
                emit(&mut chunk, true, &mut out);
                quoted = false;
            } else {
                emit(&mut chunk, false, &mut out);
                quoted = true;
                chunk.push(c);
            }
            continue;
        }
        chunk.push(c);
        if matches!(c, '.' | '!' | '?' | '…')
            && !chars
                .peek()
                .is_some_and(|c| matches!(c, '.' | '!' | '?' | '…' | '"' | '\''))
        {
            emit(&mut chunk, quoted, &mut out);
        }
    }
    emit(&mut chunk, quoted, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_recipe_gem_spark_and_boom() {
        let mut synth = Synth::default();
        assert_eq!(synth.recipe(Effect::Gem(999))[0].frequency, 523.25 * 4.);
        assert_eq!(synth.recipe(Effect::Breath(Breath::Spark)).len(), 1);
        assert!(synth.recipe(Effect::Breath(Breath::Spark)).is_empty());
        let boom = synth.recipe(Effect::MegaBoom);
        assert_eq!(boom.len(), 4);
        assert_eq!(boom[0].duration, 1.6);
        assert_eq!(boom[1].end_frequency, Some(28.));
        assert_eq!(boom[3].delay, 0.1);
        let source = include_str!("../../web/dragon/index.html");
        assert!(source.contains("noise(1.6, 0.6, 180); tone(70, 1.2, 'sine', 0.4, 28)"));
    }
    #[test]
    fn pcm_is_valid_nonempty_and_repeatable() {
        let notes = Synth::default().recipe(Effect::Win);
        let bytes = wav(&notes);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes, wav(&notes));
        assert!(bytes[44..].iter().any(|&b| b != 0));
    }
    #[test]
    fn music_source_first_chord_and_stop() {
        let mut music = Music::default();
        let notes = music.tick(0., MusicScene::Realm(4), false, || 0.5);
        assert_eq!(notes.iter().filter(|n| n.delay == 0.15).count(), 6); // three pad notes, bass, lead, kick
        assert_eq!(notes[0].delay, 0.15);
        assert_eq!(notes[0].volume, 0.03);
        assert!(
            music
                .tick(0.1, MusicScene::Silent, false, || 0.5)
                .is_empty()
        );
    }
    #[test]
    fn narrator_preserves_character_and_excited_voice() {
        let p = narration_pieces("Hello! \"Come here.\" Then we fly.");
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].rate, 1.04);
        assert!(p[1].quoted);
        assert_eq!(p[1].text, "\"Come here.\"");
        assert_eq!(p[1].pitch, 0.92);
        assert_eq!(p[2].text, "Then we fly.");
    }
}

/// Cached SFX recipes (including the alternating spark) and a hard playback cap.
/// Call once per frame with current ValmoraSound entities, then enqueue music/SFX.
#[derive(Default)]
pub struct Playback {
    synth: Synth,
    #[cfg(not(target_arch = "wasm32"))]
    cache: Vec<(Effect, bevy::prelude::Handle<bevy::audio::AudioSource>)>,
    voices: usize,
    #[cfg(not(target_arch = "wasm32"))]
    worker: Option<worker::Worker>,
    #[cfg(not(target_arch = "wasm32"))]
    worker_started: bool,
}
impl Playback {
    pub fn begin_frame(&mut self, live_voices: usize) {
        self.voices = live_voices;
    }
    pub fn effect(
        &mut self,
        effect: Effect,
        commands: &mut bevy::prelude::Commands,
        assets: &mut bevy::prelude::Assets<bevy::audio::AudioSource>,
    ) {
        let effect = match effect {
            Effect::Gem(n) => Effect::Gem(n.min(10)),
            other => other,
        };
        if self.voices >= 32 {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = assets;
            let notes = self.synth.recipe(effect);
            if !notes.is_empty() && web::play(&notes, commands) {
                self.voices += 1;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // The source alternates spark emissions even when the sound is already cached.
            if effect == Effect::Breath(Breath::Spark) && self.synth.recipe(effect).is_empty() {
                return;
            }
            let handle =
                if let Some((_, handle)) = self.cache.iter().find(|(key, _)| *key == effect) {
                    handle.clone()
                } else {
                    let recipe = if effect == Effect::Breath(Breath::Spark) {
                        vec![tone(1300., 0.04, Wave::Square, 0.025, Some(900.), 0.)]
                    } else {
                        self.synth.recipe(effect)
                    };
                    let handle = assets.add(bevy::audio::AudioSource {
                        bytes: wav(&recipe).into(),
                    });
                    // Gem combos are clamped before caching: finite variants only.
                    if self.cache.len() < 64 {
                        self.cache.push((effect, handle.clone()));
                    }
                    handle
                };
            commands.spawn((
                bevy::audio::AudioPlayer::new(handle),
                bevy::audio::PlaybackSettings::DESPAWN,
                ValmoraSound,
            ));
            self.voices += 1;
        }
    }
    /// Invalidates queued native synthesis; caller also despawns ValmoraMusic entities.
    pub fn cancel_music(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
    }
    pub fn music(
        &mut self,
        notes: &[Note],
        commands: &mut bevy::prelude::Commands,
        assets: &mut bevy::prelude::Assets<bevy::audio::AudioSource>,
    ) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = assets;
            if self.voices < 32 && !notes.is_empty() && web::play(notes, commands) {
                self.voices += 1;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if !self.worker_started && !notes.is_empty() {
                self.worker_started = true;
                self.worker = worker::Worker::new();
            }
            let Some(worker) = &self.worker else {
                return;
            };
            if self.voices < 32 {
                worker.submit(notes);
            }
            // At most one result/asset/spawn per frame; bytes are already Arc-backed.
            if let Some(ready) = worker.take() {
                let elapsed = ready.submitted.elapsed();
                // Bevy skip_duration decodes skipped samples on setup. Cap that work
                // at 4,410 samples; under overload drop the batch, never play it late.
                if self.voices >= 32
                    || elapsed.as_secs_f64() > 0.1
                    || elapsed.as_secs_f64() >= ready.duration
                {
                    return;
                }
                let source = assets.add(bevy::audio::AudioSource { bytes: ready.bytes });
                commands.spawn((
                    bevy::audio::AudioPlayer::new(source),
                    bevy::audio::PlaybackSettings::DESPAWN.with_start_position(elapsed),
                    ValmoraSound,
                    ValmoraMusic,
                ));
                self.voices += 1;
            }
        }
    }
}
/// Only map actual source sound emissions; reward/stat events alone are silent.
pub fn adventure_effect(event: &dragon_game::adventure::Event) -> Option<Effect> {
    use dragon_game::{
        adventure::Event as A,
        progression::Event as P,
        roost::{CompletionEvent as C, Event as R},
    };
    match event {
        A::Sound { name }
        | A::Roost {
            value: R::Sound { name },
        } => named_effect(name),
        A::Progression {
            value:
                P::Growth {
                    stage_before,
                    stage_after,
                    ..
                },
        } if stage_after > stage_before => Some(Effect::Grow),
        A::Progression {
            value: P::Learned { .. },
        } => Some(Effect::Grow),
        A::Progression {
            value: P::Crafted { item },
        } => match item.as_str() {
            "treatsMade" | "tea" | "tonic" => Some(Effect::Spell),
            "saddles" | "armor" | "bellows" => Some(Effect::Smash),
            _ => None,
        },
        A::Progression {
            value: P::Selected { .. },
        } => Some(Effect::Pick),
        A::Completion {
            value: C::Daily { .. },
        } => Some(Effect::Win),
        _ => None,
    }
}
pub fn named_effect(name: &str) -> Option<Effect> {
    Some(match name {
        "gold" => Effect::Gold,
        "thud" => Effect::Thud,
        "poof" => Effect::Poof,
        "grr" => Effect::Grr,
        "throw" => Effect::Throw,
        "zap" => Effect::Zap,
        "bonk" => Effect::Bonk,
        "hatch" => Effect::Hatch,
        "grow" => Effect::Grow,
        "wave" => Effect::Wave,
        "win" => Effect::Win,
        "pick" => Effect::Pick,
        "spell" => Effect::Spell,
        "shadow" => Effect::Shadow,
        "dadBoom" => Effect::DadBoom,
        "megaLaunch" => Effect::MegaLaunch,
        "megaBoom" => Effect::MegaBoom,
        "rimshot" => Effect::Rimshot,
        "smash" => Effect::Smash,
        "wiggle" => Effect::Wiggle,
        "correct" => Effect::Correct,
        "wrong" => Effect::Wrong,
        "freeze" => Effect::Freeze,
        "thaw" => Effect::Thaw,
        _ => return None,
    })
}

pub mod narration;
#[cfg(target_arch = "wasm32")]
thread_local! {static NARRATOR: std::cell::RefCell<narration::Narrator> = std::cell::RefCell::new(narration::Narrator::default());}
#[cfg(target_arch = "wasm32")]
pub fn speak(text: &str) -> bool {
    NARRATOR.with(|n| n.borrow_mut().speak(text))
}
#[cfg(target_arch = "wasm32")]
pub fn stop_speech() {
    NARRATOR.with(|n| n.borrow_mut().stop());
}
#[cfg(target_arch = "wasm32")]
#[allow(
    dead_code,
    reason = "Playback adapter for story progress and verified recorded clips"
)]
pub fn narration_progress() -> narration::Progress {
    NARRATOR.with(|n| n.borrow_mut().snapshot())
}
#[cfg(target_arch = "wasm32")]
#[allow(
    dead_code,
    reason = "Playback adapter for story progress and verified recorded clips"
)]
pub fn play_clip(url: &str, words: usize) -> bool {
    NARRATOR.with(|n| n.borrow_mut().clip(url, words))
}
#[cfg(not(target_arch = "wasm32"))]
pub fn speak(_text: &str) -> bool {
    false
}
#[cfg(not(target_arch = "wasm32"))]
pub fn stop_speech() {}
#[cfg(not(target_arch = "wasm32"))]
#[allow(
    dead_code,
    reason = "Playback adapter for story progress and verified recorded clips"
)]
pub fn narration_progress() -> narration::Progress {
    narration::Progress::default()
}
#[cfg(not(target_arch = "wasm32"))]
#[allow(
    dead_code,
    reason = "Playback adapter for story progress and verified recorded clips"
)]
pub fn play_clip(_url: &str, _words: usize) -> bool {
    false
}
#[cfg(all(test, not(target_arch = "wasm32")))]
mod oracle;
/// Exhaustive mapping of transient core-game cues; no simulation state is read.
pub fn game_effect(event: dragon_game::game::sound::Event) -> Effect {
    use dragon_game::{
        combat::{Kind as K, Path as P},
        game::sound::Event as S,
    };
    match event {
        S::Gem(n) => Effect::Gem(usize::from(n)),
        S::Breath(kind) => Effect::Breath(match kind {
            K::Spark => Breath::Spark,
            K::Boulder => Breath::Boulder,
            K::Frost => Breath::Frost,
            K::Star => Breath::Star,
            _ => Breath::Fire,
        }),
        S::Weapon(path) => Effect::Weapon(match path {
            P::Shadow => Path::Shadow,
            P::Storm => Path::Storm,
            P::Guardian => Path::Guardian,
            P::Forge => Path::Other,
        }),
        S::Gold => Effect::Gold,
        S::Thud => Effect::Thud,
        S::Poof => Effect::Poof,
        S::Grr => Effect::Grr,
        S::Throw => Effect::Throw,
        S::Zap => Effect::Zap,
        S::Bonk => Effect::Bonk,
        S::Grow => Effect::Grow,
        S::Wave => Effect::Wave,
        S::Win => Effect::Win,
        S::Wrong => Effect::Wrong,
        S::Spell => Effect::Spell,
        S::Shadow => Effect::Shadow,
        S::Smash => Effect::Smash,
        S::DadBoom => Effect::DadBoom,
        S::Rimshot => Effect::Rimshot,
        S::MegaLaunch => Effect::MegaLaunch,
        S::MegaBoom => Effect::MegaBoom,
    }
}
