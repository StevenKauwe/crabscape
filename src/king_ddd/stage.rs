//! A real Bevy court above the ocean. It uses the same separate-scene convention as the shell interior.
use super::Court;
use bevy::prelude::*;
pub const ORIGIN: Vec2 = Vec2::new(0., 40000.);
#[derive(Component)]
struct CourtScenery;
#[derive(Component)]
struct CourtKnight {
    slot: usize,
    challenger: bool,
}
#[derive(Clone)]
struct Resident {
    row: Vec<String>,
    chance: f64,
    side: usize,
}
struct Battle {
    incoming: Resident,
    incoming_wins: bool,
}
#[derive(Resource, Default)]
struct Stage {
    spawned: bool,
    clips: Vec<Vec<Handle<Image>>>,
    residents: Vec<Option<Resident>>,
    battles: Vec<Option<Battle>>,
    run_id: u64,
    elapsed: f32,
}
#[derive(serde::Deserialize)]
struct Clip {
    frames: Vec<String>,
}
pub fn install(app: &mut App) {
    app.init_resource::<Stage>().add_systems(
        Update,
        (spawn, animate)
            .chain()
            .in_set(crate::player::GameSet::Present),
    );
}
fn spawn(
    mut commands: Commands,
    court: Res<Court>,
    mut stage: ResMut<Stage>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    if !court.active || stage.spawned {
        return;
    }
    stage.spawned = true;
    stage.residents = vec![None; 6];
    stage.battles = (0..6).map(|_| None).collect();
    let manifest: std::collections::BTreeMap<String, std::collections::BTreeMap<String, Clip>> =
        serde_json::from_str(include_str!("../../assets/king-ddd/fighters/manifest.json"))
            .unwrap_or_default();
    for side in ["steel", "gold"] {
        for clip in ["idle", "slash", "block", "victory", "dieBack"] {
            stage.clips.push(
                manifest
                    .get(side)
                    .and_then(|c| c.get(clip))
                    .map(|c| {
                        c.frames
                            .iter()
                            .map(|f| server.load(format!("king-ddd/fighters/{f}")))
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
    }
    commands.spawn((
        CourtScenery,
        Sprite {
            image: server.load("king-ddd/fighters/battlefield.jpg"),
            custom_size: Some(Vec2::new(1672., 941.)),
            ..default()
        },
        Transform::from_translation((ORIGIN + Vec2::new(0., 100.)).extend(31.)),
        Visibility::Hidden,
    ));
    // The throne is painted in individual shapes, never a filter over the world.
    for (pos, size, color, z) in [
        (
            Vec2::new(0., 100.),
            Vec2::new(220., 220.),
            Color::srgb(0.20, 0.07, 0.08),
            32.,
        ),
        (
            Vec2::new(0., -4.),
            Vec2::new(270., 34.),
            Color::srgb(0.55, 0.36, 0.12),
            33.,
        ),
    ] {
        commands.spawn((
            CourtScenery,
            Sprite::from_color(color, size),
            Transform::from_translation((ORIGIN + pos).extend(z)),
            Visibility::Hidden,
        ));
    }
    for (size, pos, color, z) in [
        (
            Vec2::new(112., 154.),
            Vec2::new(0., 100.),
            Color::srgb(0.08, 0.10, 0.14),
            34.,
        ),
        (
            Vec2::new(75., 112.),
            Vec2::new(0., 85.),
            Color::srgb(0.91, 0.87, 0.73),
            35.,
        ),
        (
            Vec2::new(34., 17.),
            Vec2::new(0., 140.),
            Color::srgb(0.88, 0.53, 0.15),
            36.,
        ),
    ] {
        commands.spawn((
            CourtScenery,
            Mesh2d(meshes.add(Ellipse::new(size.x / 2., size.y / 2.))),
            MeshMaterial2d(materials.add(color)),
            Transform::from_translation((ORIGIN + pos).extend(z)),
            Visibility::Hidden,
        ));
    }
    for x in [-22., 0., 22.] {
        commands.spawn((
            CourtScenery,
            Sprite::from_color(Color::srgb(0.95, 0.72, 0.18), Vec2::new(16., 32.)),
            Transform::from_translation((ORIGIN + Vec2::new(x, 189.)).extend(36.)),
            Visibility::Hidden,
        ));
    }
    for x in [-17., 17.] {
        commands.spawn((
            CourtScenery,
            Sprite::from_color(Color::srgb(0.95, 0.91, 0.78), Vec2::new(13., 12.)),
            Transform::from_translation((ORIGIN + Vec2::new(x, 153.)).extend(36.)),
            Visibility::Hidden,
        ));
    }
    for actor in 0..12 {
        let slot = actor / 2;
        let challenger = actor % 2 == 1;
        let side = slot % 2;
        commands.spawn((
            CourtKnight { slot, challenger },
            Sprite {
                image: stage
                    .clips
                    .get(side * 5)
                    .and_then(|c| c.first())
                    .cloned()
                    .unwrap_or_default(),
                custom_size: Some(Vec2::new(130., 163.)),
                ..default()
            },
            Transform::from_translation(
                (ORIGIN + Vec2::new(-500. + slot as f32 * 200., -65.))
                    .extend(40. + slot as f32 * 0.1),
            ),
            Visibility::Hidden,
        ));
    }
}
/// Keep the painted field in the top half; UI controls and survivor table occupy the rest.
pub fn framing(window: Vec2) -> (Vec2, f32) {
    let scale = (1200. / window.x.max(320.)).max(660. / window.y.max(320.));
    (ORIGIN + Vec2::new(0., -60.), scale)
}

fn animate(
    mut court: ResMut<Court>,
    mut stage: ResMut<Stage>,
    time: Res<Time>,
    mut scenery: Query<&mut Visibility, (With<CourtScenery>, Without<CourtKnight>)>,
    mut knights: Query<(&CourtKnight, &mut Sprite, &mut Transform, &mut Visibility)>,
    mut crab: Query<&mut Transform, (With<crate::player::LocalPlayer>, Without<CourtKnight>)>,
) {
    for mut v in &mut scenery {
        v.set_if_neq(if court.active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    if stage.run_id != court.run_id {
        stage.run_id = court.run_id;
        stage.residents = vec![None; 6];
        stage.battles = (0..6).map(|_| None).collect();
        stage.elapsed = 0.;
    }
    if court.active && stage.spawned {
        if let Ok(mut tr) = crab.single_mut() {
            tr.translation = (ORIGIN + Vec2::new(-220., 50.)).extend(45.);
        }
        stage.elapsed += time.delta_secs();
        if stage.elapsed > 4.8 && stage.battles.iter().any(Option::is_some) {
            for slot in 0..6 {
                if let Some(battle) = stage.battles[slot].take()
                    && battle.incoming_wins
                {
                    stage.residents[slot] = Some(battle.incoming);
                }
            }
        }
        if stage.battles.iter().all(Option::is_none)
            && let Some(round) = court.rounds.pop_front()
        {
            let mut newcomers = round
                .into_iter()
                .filter(|c| !stage.residents.iter().flatten().any(|r| r.row == c.row))
                .collect::<Vec<_>>()
                .into_iter();
            for slot in 0..6 {
                let Some(candidate) = newcomers.next() else {
                    break;
                };
                let chance = candidate.hit.unwrap_or(0.);
                if let Some(resident) = &stage.residents[slot] {
                    let incoming = Resident {
                        row: candidate.row,
                        chance,
                        side: 1 - resident.side,
                    };
                    let incoming_wins = chance > resident.chance;
                    stage.battles[slot] = Some(Battle {
                        incoming,
                        incoming_wins,
                    });
                } else {
                    stage.residents[slot] = Some(Resident {
                        row: candidate.row,
                        chance,
                        side: slot % 2,
                    });
                }
            }
            stage.elapsed = 0.;
        }
    }
    for (actor, mut sprite, mut tr, mut visibility) in &mut knights {
        let resident = stage.residents.get(actor.slot).and_then(Option::as_ref);
        let battle = stage.battles.get(actor.slot).and_then(Option::as_ref);
        let person = if actor.challenger {
            battle.map(|b| &b.incoming)
        } else {
            resident
        };
        let Some(person) = person.filter(|_| court.active) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Visible;
        let phase = (stage.elapsed - actor.slot as f32 * 0.12).max(0.);
        let winner = battle.is_some_and(|b| actor.challenger == b.incoming_wins);
        let clip = if battle.is_some() {
            if phase < 0.6 {
                0
            } else if phase < 2.2 {
                if winner { 1 } else { 2 }
            } else if winner {
                3
            } else {
                4
            }
        } else if court.running {
            0
        } else {
            3
        };
        if let Some(frames) = stage
            .clips
            .get(person.side * 5 + clip)
            .filter(|f| !f.is_empty())
        {
            let tick = (if clip == 4 { phase - 2.2 } else { phase } * 12.).max(0.) as usize;
            let frame = if clip == 4 {
                tick.min(frames.len() - 1)
            } else {
                tick % frames.len()
            };
            sprite.image = frames[frame].clone();
        }
        sprite.color = Color::WHITE.with_alpha(if battle.is_some() && !winner && phase > 3.8 {
            (4.6 - phase).clamp(0., 1.)
        } else {
            1.
        });
        tr.translation.x =
            ORIGIN.x - 500. + actor.slot as f32 * 200. + if person.side == 0 { -32. } else { 32. };
        tr.translation.y = ORIGIN.y - 55.
            + if battle.is_some() && phase < 0.6 {
                (0.6 - phase) * 60.
            } else {
                0.
            };
    }
}
