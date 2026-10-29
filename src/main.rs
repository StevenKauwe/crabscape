//! CrabScape: a side-scrolling game about a heavy, lantern-carrying crab.
//!
//! # How a Bevy game is put together (the short version)
//!
//! - An [`App`] is the game. You add *plugins* to it and call `run()`.
//! - A *plugin* bundles related setup. `DefaultPlugins` gives us a window,
//!   rendering, input and so on; each of our modules has its own plugin.
//! - *Entities* are things in the world (a rock, a crab's claw, the camera).
//!   They are just IDs with *components* (plain data structs) attached.
//! - *Resources* are one-of-a-kind shared values (the level, the tuning).
//! - *Systems* are ordinary functions that Bevy runs, at startup or every
//!   frame. Their parameters ask for the components and resources they use.
//! - *Messages* let one system tell others that something happened.
//!
//! Each frame runs in three stages (see [`player::GameSet`]): read input,
//! simulate, then present (animate, effects, camera). Lighting runs last,
//! once every piece's final position is known.
//!
//! The game *rules* (physics, the level's shape, every number that decides
//! how the crab moves) are not in here: they are the simulation, in the
//! `crabsim` crate (`crates/sim/`), plain Rust that also runs headless for
//! tests and replays (`cargo crabsim`). Everything in `src/` presents it.
//!
//! The modules:
//! - [`tuning`]: the camera and lantern numbers
//! - [`level`]: drawing the test level (sea floor, rocks, spires)
//! - [`player`]: the crabs, and ticking the simulation each frame
//! - [`power`]: the battery saver (30 frames a second)
//! - [`replay`]: F9 saves a replay of this session
//! - [`report`]: report a bug or idea from the game (typed or spoken), with the replay
//! - [`input`]: keys, controllers and touch buttons become move requests
//! - [`puppet`]: the crab's cut-out body parts and their animation
//! - [`lantern`]: the lantern and its cheap fake lighting
//! - [`coop`]: playing with friends: the invite link and the connection
//! - [`critters`]: the snapping shrimp and the moray (placeholder shapes)
//! - [`canyon`]: the canyon below the crack: its glowing sea pens, the octopus's den
//!   and the tide's turns
//! - [`deep`]: the depth counter in the pit and the canyon, and something big in the dark
//! - [`effects`]: bubbles, sand puffs and marine snow
//! - [`feel`]: the feel lab: presets for how attacks feel, and their juice
//! - [`fight`]: the shell and poise bars, the blow's flash, fight notes
//! - [`foes`]: the enemies (placeholder shapes for now) and their tells
//! - [`eggs`]: the fish eggs that give the crab more swim strokes
//! - [`impact`]: how a blow lands: shake, kick, zoom, sound, debris
//! - [`home`]: the shell-home, the glowing culture and the lump carried home
//! - [`cling`]: the clams and shell grit clinging to a crab's shell
//! - [`moult`]: the Moult button at home (until the home menu has a Moult row)
//! - [`notes`]: the short notes that float up when something happens
//! - [`reset`]: starting over: forget this device's saved data, start fresh
//! - [`save`]: remembering the crab's position between visits
//! - [`settings`]: the settings panel: battery saver, art style, how voices carry, start over
//! - [`secrets`]: places hidden in darkness until a bright lantern comes near
//! - [`sound`]: impact sounds, synthesised in code, played with Web Audio
//! - [`stamina`]: the rings over the crab that show its swimming stamina
//! - [`style`]: the ink or oil-paint art style (P switches)
//! - [`view`]: the camera and the HUD text
//! - [`voice`]: voice chat with friends in a co-op room

mod background;
mod build;
mod canyon;
mod cling;
mod codes;
mod coop;
mod creature_art;
mod crevices;
mod critters;
mod deep;
mod effects;
mod eggs;
mod ending;
mod feel;
mod fight;
mod foes;
mod food;
mod home;
mod impact;
mod input;
mod lantern;
mod level;
mod memory;
mod moult;
mod notes;
mod player;
mod power;
mod puppet;
mod replay;
mod report;
mod reset;
mod save;
mod secrets;
mod settings;
mod sound;
mod stamina;
mod style;
mod tuning;
mod view;
mod voice;

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    // The pictures sit next to the page in `assets/`. Don't
                    // ask the server for `.meta` files we never write.
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "CrabScape".into(),
                        // In the browser, draw into the page's <canvas id="crabscape">
                        // and keep it the size of its parent element.
                        canvas: Some("#crabscape".into()),
                        fit_canvas_to_parent: true,
                        // Stop the browser scrolling or zooming the page when the
                        // player taps or drags on the game.
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                })
                // Keep the last log lines for players' bug reports.
                .set(bevy::log::LogPlugin {
                    custom_layer: report::keep_log,
                    ..default()
                }),
        )
        .init_resource::<tuning::ViewTuning>()
        // The game's core, then the world's things and the HUD. (Bevy takes
        // at most 15 plugins in one group.)
        .add_plugins((
            background::BackgroundPlugin,
            coop::CoopPlugin,
            replay::ReplayPlugin,
            memory::MemoryPlugin,
            level::LevelPlugin,
            player::PlayerPlugin,
            power::PowerPlugin,
            settings::SettingsPlugin,
            input::InputPlugin,
            puppet::PuppetPlugin,
            lantern::LanternPlugin,
            effects::EffectsPlugin,
            save::SavePlugin,
            style::StylePlugin,
            view::ViewPlugin,
        ))
        .add_plugins((
            critters::CrittersPlugin,
            deep::DeepPlugin,
            (
                eggs::EggsPlugin,
                food::FoodPlugin,
                crevices::CrevicesPlugin,
                build::BuildPlugin,
                cling::ClingPlugin,
            ),
            feel::FeelPlugin,
            foes::FoesPlugin,
            fight::FightPlugin,
            home::HomePlugin,
            impact::ImpactPlugin,
            notes::NotesPlugin,
            report::ReportPlugin,
            (
                secrets::SecretsPlugin,
                canyon::CanyonPlugin,
                ending::EndingPlugin,
                codes::CodesPlugin,
                moult::MoultPlugin,
                creature_art::CreatureArtPlugin,
            ),
            sound::SoundPlugin,
            stamina::StaminaPlugin,
            voice::VoicePlugin,
        ))
        .run();
}
