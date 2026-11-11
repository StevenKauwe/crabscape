//! Small ink-edged rosettes tied to the shell, not a marker floating above it.
//! Generated once in Rust; each dyed ribbon has its own clasp silhouette.
use super::*;

#[derive(Resource)]
pub(super) struct RibbonArt([Handle<Image>; 4]);

#[derive(Component)]
pub(super) struct RoyalRibbon(Entity);

pub(super) fn prepare(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(RibbonArt(std::array::from_fn(|i| {
        images.add(Image::new(
            Extent3d {
                width: 96,
                height: 96,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels(i),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        ))
    })));
}

fn shape(x: f32, y: f32, style: usize) -> bool {
    // Two asymmetric, notched tails below the clasp.
    let tail = (34.0..82.0).contains(&y)
        && ((x - (37.0 - (y - 34.0) * 0.24)).abs() < 9.0 && y < 82.0 - (x - 24.0).abs() * 0.65
            || (x - (56.0 + (y - 34.0) * 0.20)).abs() < 8.0 && y < 77.0 - (x - 64.0).abs() * 0.6);
    let dx = x - 47.0;
    let dy = y - 30.0;
    let angle = dy.atan2(dx);
    let radius = match style {
        0 => 18.0 + 3.0 * (angle * 5.0).cos(),
        1 => 21.0 / (angle.cos().abs() + angle.sin().abs()),
        2 => 19.0 + 3.0 * (angle * 8.0).cos(),
        _ => 17.0 + 4.0 * (angle * 4.0).cos(),
    };
    tail || dx * dx + dy * dy <= radius * radius
}

fn pixels(style: usize) -> Vec<u8> {
    let dye = [
        [53_u8, 137, 87],
        [177, 52, 57],
        [216, 159, 52],
        [133, 82, 167],
    ][style];
    let mut out = vec![0; 96 * 96 * 4];
    for y in 0..96 {
        for x in 0..96 {
            let (xf, yf) = (x as f32, y as f32);
            if !shape(xf, yf, style) {
                continue;
            }
            let edge = [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0)]
                .iter()
                .any(|(dx, dy)| !shape(xf + dx, yf + dy, style));
            let clasp = (xf - 47.0).powi(2) + (yf - 30.0).powi(2) < 37.0;
            // Brush-sized streaks stay inside the cut-out. No screen-space filter.
            let grain = ((x / 3 + y / 9 * 7 + style * 11) % 9) as i16 - 4;
            let fold = if (x + y / 4) % 17 < 4 { 22 } else { 0 };
            let color = if edge {
                [36, 30, 29]
            } else if clasp {
                [239, 210, 139]
            } else {
                dye.map(|v| (i16::from(v) + grain * 2 + fold).clamp(0, 255) as u8)
            };
            let at = (y * 96 + x) * 4;
            out[at..at + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    out
}

pub(super) fn spawn(commands: &mut Commands, crab: Entity, dorsal: Entity) {
    commands.spawn((
        ChildOf(dorsal),
        RoyalRibbon(crab),
        Sprite {
            custom_size: Some(Vec2::splat(30.0)),
            ..default()
        },
        Transform::from_xyz(16.0, 5.0, 0.008),
        Visibility::Hidden,
        Lit::at(Vec2::ZERO),
        LitTint {
            dark: Color::srgb(0.12, 0.16, 0.20),
            bright: Color::WHITE,
        },
    ));
}

pub(super) fn show(
    sim: Res<Sim>,
    art: Res<RibbonArt>,
    crabs: Query<&crabsim::PlayerId>,
    mut ribbons: Query<(&RoyalRibbon, &mut Sprite, &mut Visibility)>,
) {
    for (ribbon, mut sprite, mut visible) in &mut ribbons {
        let wearing = crabs
            .get(ribbon.0)
            .ok()
            .and_then(|id| sim.crab(*id))
            .and_then(|c| c.royal.wearing)
            .filter(|i| *i < 4);
        *visible = if let Some(i) = wearing {
            sprite.image = art.0[usize::from(i)].clone();
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_inked_ribbons_have_distinct_silhouettes_and_transparent_margins() {
        let art = std::array::from_fn::<_, 4, _>(pixels);
        for (i, bytes) in art.iter().enumerate() {
            assert_eq!(bytes.len(), 96 * 96 * 4);
            assert!(
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[3] == 255)
                    .count()
                    > 1500
            );
            assert_eq!(&bytes[..4], &[0, 0, 0, 0]);
            for other in &art[..i] {
                assert_ne!(bytes, other);
                assert!(
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(other.as_chunks::<4>().0.iter())
                        .any(|(a, b)| a[3] != b[3])
                );
            }
        }
    }
    #[test]
    #[ignore = "writes a headless art contact sheet to a caller-selected path"]
    fn export_ribbon_contact_sheet() {
        let path = std::env::var("RIBBON_PREVIEW").expect("set RIBBON_PREVIEW");
        let mut sheet = vec![0_u8; 384 * 96 * 4];
        for i in 0..4 {
            let art = pixels(i);
            for y in 0..96 {
                let to = (y * 384 + i * 96) * 4;
                sheet[to..to + 96 * 4].copy_from_slice(&art[y * 96 * 4..(y + 1) * 96 * 4]);
            }
        }
        Image::new(
            Extent3d {
                width: 384,
                height: 96,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            sheet,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        )
        .try_into_dynamic()
        .unwrap()
        .save(path)
        .unwrap();
    }

    #[test]
    fn each_crab_uses_its_own_equipment_outside_castle_and_unequip_hides_it() {
        let mut app = App::new();
        let mut sim = Sim::with_one_crab(Vec2::ZERO);
        sim.add_remote_crab(crabsim::PlayerId(1), Vec2::ZERO);
        sim.crab_mut(crabsim::PlayerId(0)).unwrap().royal.wearing = Some(2);
        sim.crab_mut(crabsim::PlayerId(1)).unwrap().royal.wearing = Some(1);
        let mut images = Assets::<Image>::default();
        let handles = std::array::from_fn(|_| images.add(Image::default()));
        app.insert_resource(RibbonArt(handles.clone()))
            .insert_resource(sim)
            .add_systems(Update, show);
        let marks = [0, 1].map(|id| {
            let crab = app.world_mut().spawn(crabsim::PlayerId(id)).id();
            app.world_mut()
                .spawn((RoyalRibbon(crab), Sprite::default(), Visibility::Hidden))
                .id()
        });
        app.update();
        for (e, index) in marks.into_iter().zip([2, 1]) {
            assert_eq!(app.world().get::<Sprite>(e).unwrap().image, handles[index]);
            assert_eq!(
                *app.world().get::<Visibility>(e).unwrap(),
                Visibility::Inherited
            );
        }
        app.world_mut()
            .resource_mut::<Sim>()
            .crab_mut(crabsim::PlayerId(0))
            .unwrap()
            .royal
            .wearing = None;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(marks[0]).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *app.world().get::<Visibility>(marks[1]).unwrap(),
            Visibility::Inherited
        );
    }
}
