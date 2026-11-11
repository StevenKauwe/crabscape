//! Explicit-file native adventure harness. Never writes the supplied save.
#[path = "../valmora/mod.rs"]
mod valmora;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use dragon_game::{flight::Viewport, profile, save::BrowserSave};
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() {
        let mut state = valmora::Valmora::default();
        match state.open_new(2, 42) {
            Ok(()) => run(state),
            Err(error) => eprintln!("Cannot start: {error}"),
        }
        return;
    }
    if args.len() != 2 {
        eprintln!(
            "Usage: dragon_adventure <explicit-profile.json> <alexa|tavian|rider3|rider4|rider5|rider6>"
        );
        return;
    }
    let slots = ["alexa", "tavian", "rider3", "rider4", "rider5", "rider6"];
    let Some(rider) = slots.iter().position(|s| *s == args[1]) else {
        eprintln!("Unknown rider slot");
        return;
    };
    let result = (|| -> Result<valmora::Valmora, String> {
        let raw = std::fs::read_to_string(&args[0]).map_err(|e| e.to_string())?;
        let save = BrowserSave::parse(&raw).map_err(|e| format!("{e:?}"))?;
        if save.raw()["rider"]
            .as_str()
            .is_some_and(|key| key != slots[rider])
        {
            return Err("The selected rider slot does not match this profile".into());
        }
        let mut fallback = 0;
        let normalized = profile::normalize_profile(save.raw(), slots[rider], rider, || {
            fallback += 1;
            format!("native-preview-extra-{fallback}")
        })
        .map_err(|e| format!("{e:?}"))?;
        let mut state = valmora::Valmora::default();
        state.open(
            normalized,
            rider,
            Viewport::from_screen(1000., 700., 1., 130., 0.62),
            42,
        )?;
        Ok(state)
    })();
    let state = match result {
        Ok(state) => state,
        Err(error) => {
            eprintln!("Cannot open profile: {error}");
            return;
        }
    };
    run(state);
}
fn run(mut state: valmora::Valmora) {
    state.enter();
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin::default())
        .insert_resource(state)
        .add_plugins(valmora::ValmoraPlugin)
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .run();
}
