//! Breath and rider-weapon patterns ported from the browser game.
//! Random spread is supplied by the caller so a recorded draw stream can be replayed.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bond,
    Flame,
    Spark,
    Boulder,
    Frost,
    Star,
    Arrow,
    Hammer,
    Area,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub life: f64,
    pub max: f64,
    pub r: f64,
    pub dmg: f64,
    pub kind: Kind,
    pub owner: String,
    pub dad: bool,
    pub pierce: bool,
    pub hits: BTreeSet<u64>,
    pub spin: f64,
    pub age: f64,
    pub slow: f64,
    pub stun: f64,
    pub homing: bool,
    pub mark: bool,
    pub crack: bool,
    pub mega: bool,
    pub spear: bool,
}
impl Default for Shot {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            life: 0.0,
            max: 0.0,
            r: 0.0,
            dmg: 0.0,
            kind: Kind::Bond,
            owner: "bond".into(),
            dad: false,
            pierce: false,
            hits: BTreeSet::new(),
            spin: 0.0,
            age: 0.0,
            slow: 0.0,
            stun: 0.0,
            homing: false,
            mark: false,
            crack: false,
            mega: false,
            spear: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Mouth {
    pub x: f64,
    pub y: f64,
    pub facing: f64,
}
#[derive(Clone, Debug)]
pub struct Breath {
    pub kind: Kind,
    pub stage: usize,
    pub owner: String,
    pub follower: bool,
    pub skill: u8,
    pub bellows: f64,
    pub dad: bool,
}
impl Breath {
    pub fn cooldown(&self) -> f64 {
        match self.kind {
            Kind::Bond => [0.42, 0.36, 0.32, 0.3][self.stage.min(3)],
            Kind::Flame => 0.42,
            Kind::Spark => 0.11,
            Kind::Boulder => 0.95,
            Kind::Frost => 0.38,
            Kind::Star => 0.3,
            _ => 0.0,
        }
    }
    pub fn emit(
        &self,
        mouth: Mouth,
        scale: f64,
        velocity: [f64; 2],
        mut random: impl FnMut() -> f64,
    ) -> Vec<Shot> {
        let st = self.stage.min(3);
        let sc = [0.62, 0.8, 1.0, 1.18][st];
        let power = (1.0 + 0.2 * self.bellows)
            * if self.follower { 0.5 } else { 1.0 }
            * if self.skill >= 1 { 1.15 } else { 1.0 };
        let mut shots = Vec::new();
        let offsets: &[f64] = if !self.follower && self.skill >= 5 && self.kind != Kind::Bond {
            &[0.0, -0.22, 0.22]
        } else {
            &[0.0]
        };
        for (index, off) in offsets.iter().enumerate() {
            let mul = if index == 0 { 1.0 } else { 0.6 };
            let (angles, speed, life, radius, damage) = match self.kind {
                Kind::Bond => {
                    let angles = if !self.follower && self.skill >= 5 {
                        if st == 3 {
                            vec![-0.3, -0.15, 0.0, 0.15, 0.3]
                        } else {
                            vec![-0.18, 0.0, 0.18]
                        }
                    } else if st == 3 {
                        vec![-0.18, 0.0, 0.18]
                    } else {
                        vec![0.0]
                    };
                    (
                        angles,
                        scale * [7.0, 9.0, 11.0, 12.0][st],
                        [0.35, 0.6, 0.95, 1.0][st],
                        scale * [0.12, 0.16, 0.2, 0.24][st],
                        [0.6, 0.8, 1.0, 1.2][st],
                    )
                }
                Kind::Flame => (
                    if st >= 2 {
                        vec![-0.4, -0.2, 0.0, 0.2, 0.4]
                    } else {
                        vec![-0.28, 0.0, 0.28]
                    },
                    scale * 9.0,
                    0.5 + 0.1 * st as f64,
                    scale * 0.16 * sc,
                    0.7 + 0.15 * st as f64,
                ),
                Kind::Spark => (
                    vec![-0.06 + random() * 0.12],
                    scale * 15.0,
                    0.7,
                    scale * 0.09 * sc,
                    0.35 + 0.1 * st as f64,
                ),
                Kind::Boulder => (
                    vec![0.0],
                    scale * 5.5,
                    1.6,
                    scale * 0.3 * (1.0 + 0.15 * st as f64),
                    2.0 + 0.6 * st as f64,
                ),
                Kind::Frost => (
                    vec![0.0],
                    scale * 8.0,
                    0.8,
                    scale * 0.17 * sc,
                    0.4 + 0.1 * st as f64,
                ),
                Kind::Star => (
                    vec![-0.3 + random() * 0.6],
                    scale * 10.0,
                    1.3,
                    scale * 0.16 * sc,
                    0.9 + 0.2 * st as f64,
                ),
                _ => continue,
            };
            for angle in angles {
                let a = angle + off;
                shots.push(Shot {
                    x: mouth.x,
                    y: mouth.y,
                    vx: a.cos() * speed * mouth.facing + velocity[0] * 0.3,
                    vy: a.sin() * speed + velocity[1] * 0.15,
                    life,
                    max: life,
                    r: radius
                        * if self.follower {
                            0.75
                        } else if self.skill >= 2 {
                            1.2
                        } else {
                            1.0
                        },
                    dmg: damage * power * mul,
                    kind: self.kind,
                    owner: self.owner.clone(),
                    dad: !self.follower && self.dad,
                    pierce: self.kind == Kind::Boulder,
                    slow: if self.kind == Kind::Frost { 2.5 } else { 0.0 },
                    homing: self.kind == Kind::Star,
                    ..Shot::default()
                });
            }
        }
        shots
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Shadow,
    Forge,
    Storm,
    Guardian,
}
impl Path {
    pub fn cooldown(self) -> f64 {
        match self {
            Self::Shadow => 0.55,
            Self::Forge => 1.1,
            _ => 0.8,
        }
    }
    pub fn weapon(
        self,
        player: [f64; 2],
        facing: f64,
        scale: f64,
        dragon_scale: f64,
        bellows: f64,
    ) -> Shot {
        let mut shot = Shot {
            x: player[0] + facing * scale * dragon_scale * 0.2,
            y: player[1] - scale * dragon_scale * 0.55,
            ..Shot::default()
        };
        let (speed, life, radius, damage) = match self {
            Self::Shadow => {
                shot.kind = Kind::Arrow;
                shot.mark = true;
                shot.pierce = true;
                (16.0, 0.9, 0.08, 0.8)
            }
            Self::Forge => {
                shot.kind = Kind::Hammer;
                shot.pierce = true;
                shot.vy = -scale * 1.5;
                (9.0, 1.6, 0.18, 1.2)
            }
            Self::Storm => {
                shot.kind = Kind::Arrow;
                shot.spear = true;
                shot.pierce = true;
                (13.0, 1.0, 0.1, 1.0)
            }
            Self::Guardian => {
                shot.kind = Kind::Star;
                shot.homing = true;
                (8.0, 1.4, 0.14, 0.9)
            }
        };
        shot.vx = facing * scale * speed;
        shot.life = life;
        shot.max = life;
        shot.r = scale * radius;
        shot.dmg = damage * (1.0 + 0.2 * bellows);
        shot
    }
}
