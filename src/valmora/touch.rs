//! Independent pointer capture for the two thumbs used by the original game.
use bevy_egui::egui::{Event, Pos2, Rect, TouchDeviceId, TouchId, TouchPhase};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Pause,
    Special,
    Swap,
    Spell(dragon_game::abilities::Spell),
    Select(usize),
}
type Finger = (TouchDeviceId, TouchId);
#[derive(Default)]
pub struct TouchControls {
    fingers: Vec<Finger>,
    stick: Option<(Finger, Pos2, Pos2)>,
    fire: Option<Finger>,
    pub actions: Vec<Action>,
}
impl TouchControls {
    /// Returns whether touch owns this frame, including the final release frame.
    pub fn update(
        &mut self,
        events: &[Event],
        playing: bool,
        focused: bool,
        panel: Rect,
        fire: Option<Rect>,
        actions: &[(Rect, Action)],
    ) -> bool {
        self.actions.clear();
        if !playing || !focused {
            *self = Self::default();
            return false;
        }
        let mut owns = !self.fingers.is_empty();
        for event in events {
            let Event::Touch {
                device_id,
                id,
                phase,
                pos,
                ..
            } = event
            else {
                continue;
            };
            owns = true;
            let finger = (*device_id, *id);
            match phase {
                TouchPhase::Start => {
                    if !self.fingers.contains(&finger) {
                        self.fingers.push(finger);
                    }
                    if fire.is_some_and(|rect| rect.contains(*pos)) {
                        self.fire = Some(finger);
                    } else if let Some((_, action)) =
                        actions.iter().find(|(rect, _)| rect.contains(*pos))
                    {
                        self.actions.push(*action);
                    } else if !panel.contains(*pos) {
                        // Source: any new sky touch takes over a potentially stale stick.
                        self.stick = Some((finger, *pos, *pos));
                    }
                }
                TouchPhase::Move => {
                    if let Some((owner, _, point)) = &mut self.stick
                        && *owner == finger
                    {
                        *point = *pos;
                    }
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    self.fingers.retain(|id| *id != finger);
                    if self.stick.is_some_and(|(id, _, _)| id == finger) {
                        self.stick = None;
                    }
                    if self.fire == Some(finger) {
                        self.fire = None;
                    }
                }
            }
        }
        owns
    }
    pub fn stick(&self) -> Option<(Pos2, Pos2)> {
        self.stick.map(|(_, origin, point)| (origin, point))
    }
    pub fn fire(&self) -> bool {
        self.fire.is_some()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event(id: u64, phase: TouchPhase, x: f32, y: f32) -> Event {
        Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(id),
            phase,
            pos: Pos2::new(x, y),
            force: None,
        }
    }
    fn step(t: &mut TouchControls, events: &[Event]) -> bool {
        t.update(
            events,
            true,
            true,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(300., 200.)),
            Some(Rect::from_min_max(
                Pos2::new(100., 100.),
                Pos2::new(150., 150.),
            )),
            &[],
        )
    }
    #[test]
    fn steering_and_fire_have_independent_capture() {
        let mut t = TouchControls::default();
        step(
            &mut t,
            &[
                event(1, TouchPhase::Start, 400., 400.),
                event(2, TouchPhase::Start, 125., 125.),
                event(1, TouchPhase::Move, 450., 350.),
            ],
        );
        assert!(t.fire());
        assert_eq!(
            t.stick(),
            Some((Pos2::new(400., 400.), Pos2::new(450., 350.)))
        );
        step(&mut t, &[event(2, TouchPhase::Move, 900., 900.)]);
        assert!(t.fire());
        step(&mut t, &[event(1, TouchPhase::End, 450., 350.)]);
        assert!(t.fire());
        assert!(t.stick().is_none());
        assert!(step(&mut t, &[event(2, TouchPhase::Cancel, 900., 900.)]));
        assert!(!t.fire());
        assert!(!step(&mut t, &[]));
    }
    #[test]
    fn newest_sky_touch_replaces_old_and_ui_does_not_steer() {
        let mut t = TouchControls::default();
        step(
            &mut t,
            &[
                event(1, TouchPhase::Start, 400., 400.),
                event(2, TouchPhase::Start, 500., 500.),
                event(1, TouchPhase::End, 400., 400.),
                event(3, TouchPhase::Start, 20., 20.),
            ],
        );
        assert_eq!(t.stick().unwrap().0, Pos2::new(500., 500.));
        t.update(&[], false, true, Rect::NOTHING, None, &[]);
        assert!(t.stick().is_none());
        assert!(!t.fire());
    }
    #[test]
    fn second_thumb_triggers_action_on_press_not_release() {
        let mut t = TouchControls::default();
        step(&mut t, &[event(1, TouchPhase::Start, 400., 400.)]);
        let rect = Rect::from_min_max(Pos2::new(160., 100.), Pos2::new(220., 150.));
        t.update(
            &[event(2, TouchPhase::Start, 180., 125.)],
            true,
            true,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(300., 200.)),
            None,
            &[(rect, Action::Special)],
        );
        assert_eq!(t.actions, [Action::Special]);
        assert_eq!(t.stick().unwrap().0, Pos2::new(400., 400.));
        step(&mut t, &[event(2, TouchPhase::End, 180., 125.)]);
        assert!(t.actions.is_empty());
        assert!(t.stick().is_some());
    }
    #[test]
    fn focus_loss_releases_all_controls() {
        let mut t = TouchControls::default();
        step(
            &mut t,
            &[
                event(1, TouchPhase::Start, 400., 400.),
                event(2, TouchPhase::Start, 125., 125.),
            ],
        );
        t.update(&[], true, false, Rect::NOTHING, None, &[]);
        assert!(t.stick().is_none());
        assert!(!t.fire());
    }
}
