//! Isolated browser-audio verification entry point. No game, saves, renderer, or input capture.
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)] // This focused probe deliberately imports the production adapter, not every caller.
#[path = "../valmora/audio.rs"]
mod audio;
#[cfg(target_arch = "wasm32")]
mod probe {
    use super::audio;
    use bevy::{ecs::system::RunSystemOnce, prelude::*};
    use std::cell::RefCell;
    use wasm_bindgen::prelude::*;
    struct State {
        world: World,
        assets: Assets<bevy::audio::AudioSource>,
        playback: audio::Playback,
    }
    impl Default for State {
        fn default() -> Self {
            let mut world = World::new();
            world.insert_resource(Time::<Real>::default());
            Self {
                world,
                assets: Assets::default(),
                playback: audio::Playback::default(),
            }
        }
    }
    thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
    fn with_playback(
        f: impl FnOnce(&mut audio::Playback, &mut Commands, &mut Assets<bevy::audio::AudioSource>),
    ) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let State {
                world,
                assets,
                playback,
            } = &mut *s;
            let count = world
                .query_filtered::<Entity, With<audio::ValmoraSound>>()
                .iter(world)
                .count();
            playback.begin_frame(count);
            f(playback, &mut world.commands(), assets);
            world.flush();
        });
    }
    #[wasm_bindgen]
    pub fn effect(name: &str) {
        let effect = if name == "spark" {
            Some(audio::Effect::Breath(audio::Breath::Spark))
        } else {
            audio::named_effect(name)
        };
        if let Some(effect) = effect {
            with_playback(|p, c, a| p.effect(effect, c, a));
        }
    }
    #[wasm_bindgen]
    pub fn music() {
        let notes = audio::Music::default().tick(0., audio::MusicScene::Calm, false, || 0.5);
        with_playback(|p, c, a| p.music(&notes, c, a));
    }
    #[wasm_bindgen]
    pub fn counts() -> String {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let w = &mut s.world;
            let total = w
                .query_filtered::<Entity, With<audio::ValmoraSound>>()
                .iter(w)
                .count();
            let music = w
                .query_filtered::<Entity, With<audio::ValmoraMusic>>()
                .iter(w)
                .count();
            serde_json::json!({"total":total,"music":music}).to_string()
        })
    }
    #[wasm_bindgen]
    pub fn stop(music_only: bool) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let ids: Vec<_> = if music_only {
                s.world
                    .query_filtered::<Entity, With<audio::ValmoraMusic>>()
                    .iter(&s.world)
                    .collect()
            } else {
                s.world
                    .query_filtered::<Entity, With<audio::ValmoraSound>>()
                    .iter(&s.world)
                    .collect()
            };
            for id in ids {
                s.world.despawn(id);
            }
            s.playback.cancel_music();
        });
    }
    #[wasm_bindgen]
    pub fn reap(seconds: f64) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.world
                .resource_mut::<Time<Real>>()
                .advance_by(std::time::Duration::from_secs_f64(seconds));
            s.world
                .run_system_once(audio::reap_browser_audio)
                .expect("audio reaper");
            s.world.flush();
        });
    }
    #[wasm_bindgen]
    pub fn narrate(text: &str) -> bool {
        audio::speak(text)
    }
    #[wasm_bindgen]
    pub fn cancel_narration() {
        audio::stop_speech();
    }
    #[wasm_bindgen]
    pub fn progress() -> String {
        let p = audio::narration_progress();
        serde_json::json!({"speaking":p.speaking,"word":p.word,"active":p.active_range,"done":p.done_to,"failed":p.failed}).to_string()
    }
}
fn main() {}
