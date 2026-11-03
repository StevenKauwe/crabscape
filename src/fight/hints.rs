//! Brief instructions belong to the screen, so scuttling or zooming cannot
//! leave them behind. Combat results remain floating world-space notes.
use crate::player::GameSet;
use bevy::prelude::*;

pub(super) struct HintPlugin;
impl Plugin for HintPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ShowHint>()
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                show.after(super::fight_notes).in_set(GameSet::Present),
            );
    }
}

#[derive(Message)]
pub(super) enum ShowHint {
    Encounter(&'static str),
    Grabbed,
    Released,
}

#[derive(Component, Default)]
struct Hint {
    left: f32,
    held: bool,
}
#[derive(Component)]
struct HintPosition;
const LIFE: f32 = 4.5;

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            HintPosition,
            bevy::ui::FocusPolicy::Pass,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(238),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|c| {
            c.spawn((
                Hint::default(),
                bevy::ui::FocusPolicy::Pass,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextLayout::justify(Justify::Center),
                TextColor(Color::srgb(0.92, 0.94, 0.88)),
                Node {
                    max_width: px(350),
                    width: percent(90),
                    padding: UiRect::axes(px(10), px(6)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.03, 0.08, 0.09, 0.92)),
                Visibility::Hidden,
            ));
        });
}

fn show(
    time: Res<Time>,
    windows: Query<&Window>,
    mut messages: MessageReader<ShowHint>,
    mut position: Single<&mut Node, With<HintPosition>>,
    mut hint: Single<(&mut Hint, &mut Text, &mut Visibility)>,
) {
    // Keep the phone's inventory/salvage rows above this hint. In landscape,
    // use the upper-right space below Home: inventory moves to the centre
    // at y=90, and salvage choices occupy the left 300 pixels at y=145.
    let short = windows
        .iter()
        .next()
        .is_some_and(|w| w.height() < 600.0 && w.width() > w.height());
    position.top = px(if short { 145.0 } else { 238.0 });
    position.left = px(if short { 320.0 } else { 0.0 });
    position.right = px(if short { 12.0 } else { 0.0 });
    let (state, text, visible) = &mut *hint;
    state.left = (state.left - time.delta_secs()).max(0.0);
    for message in messages.read() {
        match message {
            ShowHint::Encounter(words) if !state.held => {
                text.0 = (*words).into();
                state.left = LIFE;
            }
            ShowHint::Grabbed => {
                text.0 =
                    "Held! Tap DODGE to struggle.\nHold DODGE to drop a leg and escape.".into();
                state.held = true;
            }
            ShowHint::Released => {
                if state.held {
                    state.left = 0.0;
                }
                state.held = false;
            }
            ShowHint::Encounter(_) => {}
        }
    }
    **visible = if state.held || state.left > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn advance(app: &mut App, seconds: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(seconds));
        app.update();
    }

    #[test]
    fn instructions_stay_readable_then_expire_and_grab_help_has_priority() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .add_message::<ShowHint>()
            .add_systems(Startup, spawn)
            .add_systems(Update, show);
        let window = app
            .world_mut()
            .spawn(Window {
                resolution: (430, 742).into(),
                ..default()
            })
            .id();
        app.update();
        let hint = app
            .world_mut()
            .query_filtered::<Entity, With<Hint>>()
            .single(app.world())
            .unwrap();
        let position = app
            .world_mut()
            .query_filtered::<Entity, With<HintPosition>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            *app.world().get::<Visibility>(hint).unwrap(),
            Visibility::Hidden
        );
        app.world_mut()
            .write_message(ShowHint::Encounter("Mantis\nTap GUARD"));
        advance(&mut app, 0.0);
        advance(&mut app, 2.0);
        assert_eq!(
            *app.world().get::<Visibility>(hint).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(app.world().get::<Node>(position).unwrap().top, px(238));
        advance(&mut app, 3.0);
        assert_eq!(
            *app.world().get::<Visibility>(hint).unwrap(),
            Visibility::Hidden
        );

        app.world_mut().write_message(ShowHint::Grabbed);
        advance(&mut app, 0.0);
        let held_text = app.world().get::<Text>(hint).unwrap().0.clone();
        app.world_mut()
            .write_message(ShowHint::Encounter("Other creature"));
        advance(&mut app, 5.0);
        assert_eq!(app.world().get::<Text>(hint).unwrap().0, held_text);
        assert_eq!(
            *app.world().get::<Visibility>(hint).unwrap(),
            Visibility::Inherited
        );
        app.world_mut()
            .get_mut::<Window>(window)
            .unwrap()
            .resolution
            .set(742.0, 430.0);
        advance(&mut app, 0.0);
        assert_eq!(app.world().get::<Node>(position).unwrap().top, px(145));
        assert_eq!(app.world().get::<Node>(position).unwrap().left, px(320));
        app.world_mut().write_message(ShowHint::Released);
        advance(&mut app, 0.0);
        assert_eq!(
            *app.world().get::<Visibility>(hint).unwrap(),
            Visibility::Hidden
        );
    }
}
