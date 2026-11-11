//! Playback-only presentation adapter. It never reads a microphone or save store.
use super::{Valmora, audio};
use bevy::prelude::*;
#[derive(Default)]
pub struct Runtime {
    playback: audio::Playback,
    music: audio::Music,
    random: u32,
    active: bool,
    scene: Option<audio::MusicScene>,
    muted: bool,
}
pub fn update(
    time: Res<Time<Real>>,
    mut state: ResMut<Valmora>,
    mut runtime: Local<Runtime>,
    mut commands: Commands,
    assets: Option<ResMut<Assets<bevy::audio::AudioSource>>>,
    voices: Query<(Entity, Option<&audio::ValmoraMusic>), With<audio::ValmoraSound>>,
) {
    let Some(mut assets) = assets else {
        return;
    };
    let active = state.active;
    let muted = state.ui.sound_muted;
    let scene = if !active || muted || !state.ui.preferences.music_on {
        audio::MusicScene::Silent
    } else if let Some(adventure) = &state.adventure {
        if let Some(game) = adventure.game() {
            audio::MusicScene::Realm(game.definition.realm)
        } else {
            audio::MusicScene::Calm
        }
    } else {
        audio::MusicScene::Silent
    };
    if runtime.scene != Some(scene) {
        runtime.playback.cancel_music();
    }
    if (!active && runtime.active) || (muted && !runtime.muted) {
        for (entity, _) in &voices {
            commands.entity(entity).despawn();
        }
        if !active {
            audio::stop_speech();
        }
    } else if runtime.scene != Some(scene) {
        for (entity, music) in &voices {
            if music.is_some() {
                commands.entity(entity).despawn();
            }
        }
    }
    runtime.active = active;
    runtime.muted = muted;
    runtime.scene = Some(scene);
    runtime.playback.begin_frame(voices.iter().count());
    for effect in state.ui.sound_events.drain(..) {
        if active && !muted {
            runtime.playback.effect(effect, &mut commands, &mut assets);
        }
    }
    if let Some(adventure) = state.adventure.as_mut() {
        for event in adventure.drain_sound_events() {
            if active && !muted {
                runtime
                    .playback
                    .effect(audio::game_effect(event), &mut commands, &mut assets);
            }
        }
    }
    if let Some(narration) = state.ui.narration.take() {
        if let Some(text) = narration.filter(|_| active && state.ui.preferences.narrate_on) {
            let _ = audio::speak(&text);
        } else {
            audio::stop_speech();
        }
    }
    let boss = state
        .adventure
        .as_ref()
        .filter(|a| a.state() == "play")
        .and_then(|a| a.game())
        .is_some_and(|g| {
            g.enemies
                .iter()
                .any(|e| e.target.boss && !e.target.dead && !e.target.calm)
        });
    let mut random = runtime.random.max(1);
    let notes = runtime
        .music
        .tick(time.elapsed_secs_f64(), scene, boss, || {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            random as f64 / 4294967296.
        });
    runtime.random = random;
    runtime.playback.music(&notes, &mut commands, &mut assets);
}
