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
#[derive(serde::Deserialize)]
struct Manifest {
    steel: std::collections::BTreeMap<String, Clip>,
    gold: std::collections::BTreeMap<String, Clip>,
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
    let manifest: Manifest =
        match serde_json::from_str(include_str!("../../assets/king-ddd/fighters/manifest.json")) {
            Ok(m) => m,
            Err(e) => {
                error!("The court's knight manifest could not load: {e}");
                return;
            }
        };
    commands.spawn((
        CourtScenery,
        Sprite::from_color(Color::srgb(0.57, 0.60, 0.58), Vec2::splat(100000.)),
        Transform::from_translation(ORIGIN.extend(30.)),
        Visibility::Hidden,
    ));
    for source in [&manifest.steel, &manifest.gold] {
        for clip in ["idle", "slash", "block", "victory", "dieBack"] {
            stage.clips.push(
                source
                    .get(clip)
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
        Transform::from_translation((ORIGIN + Vec2::new(0., -65.)).extend(31.)),
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
            bevy::sprite::Anchor(Vec2::new(0., 0.5 - 462. / 560.)),
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
pub fn framing(window: Vec2, unit_px: f32, frame: Option<[f32; 4]>) -> (Vec2, f32) {
    let rect = frame
        .filter(|r| r.iter().all(|n| n.is_finite()) && r[2] > 1. && r[3] > 1.)
        .map(|r| Rect::new(r[0], r[1], r[0] + r[2], r[1] + r[3]))
        .unwrap_or_else(|| Rect::from_corners(Vec2::ZERO, window));
    let pixels_per_unit = (rect.width() / 1300.).min(rect.height() / 420.).max(0.01);
    let off = (rect.center() - window / 2.) / pixels_per_unit;
    (
        ORIGIN + Vec2::new(-off.x, 55. + off.y),
        unit_px.max(0.001) / pixels_per_unit,
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_knight_manifest_ignores_source_metadata() {
        let manifest: Manifest =
            serde_json::from_str(include_str!("../../assets/king-ddd/fighters/manifest.json"))
                .unwrap();
        for side in [&manifest.steel, &manifest.gold] {
            for name in ["idle", "slash", "block", "victory", "dieBack"] {
                let clip = side.get(name).unwrap();
                assert!(!clip.frames.is_empty());
                for frame in &clip.frames {
                    assert!(
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("assets/king-ddd/fighters")
                            .join(frame)
                            .is_file(),
                        "missing {frame}"
                    );
                }
            }
        }
    }
    #[test]
    fn camera_maps_the_court_to_the_visible_stage() {
        for (window, rect, unit_px) in [
            (Vec2::new(1200., 800.), [260., 70., 940., 330.], 2.4),
            (Vec2::new(390., 844.), [0., 490., 390., 170.], 0.78),
        ] {
            let (camera, scale) = framing(window, unit_px, Some(rect));
            let k = unit_px / scale;
            let center = ORIGIN + Vec2::new(0., 55.);
            let screen =
                window / 2. + Vec2::new((center.x - camera.x) * k, (camera.y - center.y) * k);
            assert!(
                (screen - Vec2::new(rect[0] + rect[2] / 2., rect[1] + rect[3] / 2.)).length()
                    < 0.01
            );
        }
    }
}
