//! Numerical port of `resize`, `clampPlayer` and the flight portion of `updatePlayer`.
//! Combat callbacks are returned as events. Their implementations are not ported yet.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Viewport {
    pub width: f64,
    pub scale: f64,
    pub ground_y: f64,
    pub top_limit: f64,
    pub dragon_scale: f64,
}
impl Viewport {
    /// HUD bottom is measured in screen pixels, just like the browser DOM rectangle.
    pub fn from_screen(
        width: f64,
        height: f64,
        view: f64,
        hud_bottom: f64,
        dragon_scale: f64,
    ) -> Self {
        Self {
            width: width / view,
            scale: (width.min(height) / 11.0).clamp(30.0, 64.0),
            ground_y: (height - 84.0_f64.max(height * 0.13)) / view,
            top_limit: (hud_bottom + 6.0) / view,
            dragon_scale,
        }
    }
    fn valid(self) -> bool {
        [
            self.width,
            self.scale,
            self.ground_y,
            self.top_limit,
            self.dragon_scale,
        ]
        .iter()
        .all(|x| x.is_finite())
            && self.width > 0.0
            && self.scale > 0.0
            && self.dragon_scale > 0.0
    }
    pub fn clamp(self, player: &mut Player) {
        let size = self.scale * self.dragon_scale;
        player.x = player
            .x
            .clamp(size * 0.9, (self.width - size * 0.9).max(size * 0.9));
        player.y = player.y.clamp(
            self.top_limit + size * 0.75,
            (self.ground_y - size * 0.45).max(self.top_limit + size * 0.75),
        );
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub fire: bool,
    /// Joystick displacement divided by its radius, before the 12% dead zone.
    pub stick: Option<[f64; 2]>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct FlightRules {
    pub tea: f64,
    pub magic: bool,
    pub max_energy: f64,
    pub mission_done: bool,
}
impl Default for FlightRules {
    fn default() -> Self {
        Self {
            tea: 0.0,
            magic: false,
            max_energy: 100.0,
            mission_done: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Player {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub facing: i8,
    pub safe: f64,
    pub shield: f64,
    pub sila: f64,
    pub magnet: f64,
    pub tired: f64,
    pub special_cd: f64,
    pub fire_flash: f64,
    pub main_cool: f64,
    pub weapon_cool: f64,
    pub follow_cool: BTreeMap<String, f64>,
    pub dad_cd: BTreeMap<String, f64>,
    pub mega_t: f64,
    pub energy: f64,
    pub out: bool,
    pub down: f64,
    pub hearts: u8,
    pub shadow: f64,
    pub dizzy: f64,
    pub spin: f64,
}
impl Default for Player {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            facing: 1,
            safe: 0.0,
            shield: 0.0,
            sila: 0.0,
            magnet: 0.0,
            tired: 0.0,
            special_cd: 0.0,
            fire_flash: 0.0,
            main_cool: 0.0,
            weapon_cool: 0.0,
            follow_cool: BTreeMap::new(),
            dad_cd: BTreeMap::new(),
            mega_t: 0.0,
            energy: 100.0,
            out: false,
            down: 0.0,
            hearts: 5,
            shadow: 0.0,
            dizzy: 0.0,
            spin: 0.0,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Events {
    pub mega_blast: bool,
    pub shadow_burst: bool,
    pub recovered: bool,
    pub shoot: bool,
}
impl Player {
    /// Same clamp as `frame(now)`. Hit-stop must supply zero elapsed time.
    /// Invalid external input is rejected without mutating the previous good state.
    pub fn step(
        &mut self,
        elapsed: f64,
        input: Input,
        view: Viewport,
        rules: FlightRules,
    ) -> Result<Events, &'static str> {
        if !elapsed.is_finite()
            || !view.valid()
            || !rules.tea.is_finite()
            || !rules.max_energy.is_finite()
            || input
                .stick
                .is_some_and(|v| v.iter().any(|x| !x.is_finite()))
        {
            return Err("Non-finite flight input or invalid viewport");
        }
        let dt = elapsed.clamp(0.0, 0.05);
        let mut events = Events::default();
        for timer in [
            &mut self.safe,
            &mut self.shield,
            &mut self.sila,
            &mut self.magnet,
            &mut self.tired,
            &mut self.special_cd,
            &mut self.fire_flash,
            &mut self.main_cool,
            &mut self.weapon_cool,
        ] {
            *timer -= dt;
        }
        for timer in self
            .follow_cool
            .values_mut()
            .chain(self.dad_cd.values_mut())
        {
            *timer -= dt;
        }
        if self.mega_t > 0.0 {
            self.mega_t -= dt;
            events.mega_blast = self.mega_t <= 0.0;
        }
        if self.tired <= 0.0 {
            self.energy = rules
                .max_energy
                .min(self.energy + if rules.magic { 6.5 } else { 5.0 } * dt);
        }
        if self.out {
            self.vx = 0.0;
            self.vy = 0.0;
            let k = (dt * 1.6).min(1.0);
            self.x += (view.scale * 2.2 - self.x) * k;
            self.y += (view.ground_y - view.scale * view.dragon_scale * 0.55 - self.y) * k;
            self.facing = 1;
            return Ok(events);
        }
        if self.down > 0.0 {
            self.down -= dt;
            self.vx *= 1.0 - (dt * 2.0).min(1.0);
            self.vy = (self.vy + view.scale * 3.0 * dt).min(view.scale * 1.2);
            self.x += self.vx * dt;
            self.y += self.vy * dt;
            view.clamp(self);
            if self.down <= 0.0 {
                self.hearts = 3;
                self.safe = 2.5;
                events.recovered = true;
            }
            return Ok(events);
        }
        if self.shadow > 0.0 {
            self.shadow -= dt;
            events.shadow_burst = self.shadow <= 0.0;
            // Cosmetic shadow particle RNG belongs to presentation, not this flight slice.
        }
        if self.dizzy > 0.0 {
            self.dizzy -= dt;
            self.spin += dt * 14.0;
            let k = 1.0 - (dt * 3.0).min(1.0);
            self.vx *= k;
            self.vy *= k;
            if self.dizzy <= 0.0 {
                self.spin = 0.0;
            }
        } else {
            let mut ix = i32::from(input.right) as f64 - i32::from(input.left) as f64;
            let mut iy = i32::from(input.down) as f64 - i32::from(input.up) as f64;
            if let Some([dx, dy]) = input.stick {
                let d = dx.hypot(dy);
                (ix, iy) = if d > 0.12 {
                    (dx / d * d.min(1.0), dy / d * d.min(1.0))
                } else {
                    (0.0, 0.0)
                };
            }
            let length = ix.hypot(iy);
            if length > 1.0 {
                ix /= length;
                iy /= length;
            }
            let speed = view.scale
                * 7.5
                * (1.0 + 0.08 * rules.tea)
                * if self.shadow > 0.0 { 1.25 } else { 1.0 };
            let k = (dt * 7.0).min(1.0);
            self.vx += (ix * speed - self.vx) * k;
            self.vy += (iy * speed - self.vy) * k;
            if ix.abs() > 0.15 {
                self.facing = if ix > 0.0 { 1 } else { -1 };
            }
            events.shoot = input.fire && !rules.mission_done;
        }
        self.x += self.vx * dt;
        self.y += self.vy * dt;
        view.clamp(self);
        Ok(events)
    }
}
