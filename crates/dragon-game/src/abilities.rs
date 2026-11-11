//! Special moves and Old Tongue rules from `web/dragon/index.html`.
//! Events are ordered synchronous world commands: apply Hit with `hit_enemy`, Calm
//! with the world's `defeat(id, "bond", true)`, and GemRing with `gemRing` before
//! advancing the simulation. Cast increments castCount and sets lastCast for the
//! network adapter. Presentation and its random draws are deliberately excluded.
//! Target snapshots preserve the source's `enemies.slice()` iteration; newly spawned
//! enemies from a defeat must not join the current ability's iteration.
use crate::{
    combat::{Kind, Path, Shot},
    flight::Player,
    projectiles::Target,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Spell {
    Naina,
    Thuri,
    Kaelo,
    Vara,
    Sila,
    Mira,
    Senn,
    Faro,
}
impl Spell {
    pub const ALL: [Self; 8] = [
        Self::Naina,
        Self::Thuri,
        Self::Kaelo,
        Self::Vara,
        Self::Sila,
        Self::Mira,
        Self::Senn,
        Self::Faro,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Naina => "naina",
            Self::Thuri => "thuri",
            Self::Kaelo => "kaelo",
            Self::Vara => "vara",
            Self::Sila => "sila",
            Self::Mira => "mira",
            Self::Senn => "senn",
            Self::Faro => "faro",
        }
    }
    pub fn mission(self) -> Option<&'static str> {
        match self {
            Self::Naina => None,
            Self::Thuri => Some("m2"),
            Self::Kaelo => Some("m3"),
            Self::Vara => Some("m5"),
            Self::Sila => Some("m6"),
            Self::Mira => Some("m7"),
            Self::Senn => Some("m9"),
            Self::Faro => Some("m11"),
        }
    }
    pub fn cost(self, magic_skill: u8) -> f64 {
        let base = match self {
            Self::Naina => 50.0,
            Self::Thuri => 55.0,
            Self::Kaelo => 35.0,
            Self::Vara | Self::Sila => 40.0,
            Self::Mira | Self::Senn => 60.0,
            Self::Faro => 90.0,
        };
        (base * if magic_skill >= 2 { 0.8_f64 } else { 1.0 }).round()
    }
    pub fn team(self) -> bool {
        matches!(self, Self::Naina | Self::Thuri)
    }
}
pub fn spell_slots(rank: usize) -> usize {
    if rank >= 3 { 3 } else { 2 }
}
pub fn known_words(cleared: &[String]) -> Vec<Spell> {
    Spell::ALL
        .into_iter()
        .filter(|s| s.mission().is_none_or(|m| cleared.iter().any(|c| c == m)))
        .collect()
}
/// Preserves source loadout order and duplicates; uses known-word order only if
/// no valid selections remain. Caller passes None when the profile is absent.
pub fn active_words(profile: Option<(&[String], &[Spell], usize)>) -> Vec<Spell> {
    let Some((cleared, loadout, rank)) = profile else {
        return vec![];
    };
    let known = known_words(cleared);
    let slots = spell_slots(rank);
    let picked: Vec<_> = loadout
        .iter()
        .copied()
        .filter(|s| known.contains(s))
        .take(slots)
        .collect();
    if picked.is_empty() {
        known.into_iter().take(slots).collect()
    } else {
        picked
    }
}
pub fn spell_power(magic_skill: u8) -> f64 {
    if magic_skill >= 5 { 1.5 } else { 1.0 }
}
pub fn max_energy(magic_skill: u8) -> f64 {
    if magic_skill >= 4 { 130.0 } else { 100.0 }
}
#[derive(Clone, Debug)]
pub struct Rules {
    pub playing: bool,
    pub has_player: bool,
    pub daddio: bool,
    pub path: Path,
    pub rider_skill: u8,
    pub magic_skill: u8,
    pub bellows: f64,
    pub scale: f64,
    /// Result of maxHearts: includes armor, rider skill, nearby dad and charm.
    pub max_hearts: u8,
    pub active_words: Vec<Spell>,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            playing: true,
            has_player: true,
            daddio: false,
            path: Path::Forge,
            rider_skill: 0,
            magic_skill: 0,
            bellows: 0.0,
            scale: 40.0,
            max_hearts: 5,
            active_words: vec![Spell::Naina],
        }
    }
}
impl Rules {
    pub fn special_cooldown(&self) -> f64 {
        if self.daddio {
            30.0
        } else if self.rider_skill >= 3 {
            9.0
        } else {
            12.0
        }
    }
    fn bellows_mult(&self) -> f64 {
        1.0 + 0.2 * self.bellows
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Hit { id: u64, shot: Shot },
    Calm { id: u64 },
    GemRing { count: u8, rich: bool },
    Cast { key: String },
    Rain { duration: f64 },
    TooTired,
}
fn hit(id: u64, dmg: f64, stun: f64, mark: bool, crack: bool, mega: bool) -> Event {
    Event::Hit {
        id,
        shot: Shot {
            dmg,
            stun,
            mark,
            crack,
            mega,
            kind: Kind::Area,
            ..Shot::default()
        },
    }
}
fn area(
    p: &Player,
    rules: &Rules,
    targets: &[Target],
    reach: f64,
    dmg: f64,
    mark: bool,
    crack: bool,
) -> Vec<Event> {
    targets
        .iter()
        .filter(|e| !e.dead && (e.x - p.x).hypot(e.y - p.y) < rules.scale * reach + e.r * 0.6)
        .map(|e| hit(e.id, dmg * rules.bellows_mult(), 0.0, mark, crack, false))
        .collect()
}
fn gems(events: &mut Vec<Event>, rs: u8) {
    if rs >= 5 {
        events.push(Event::GemRing {
            count: 4,
            rich: false,
        });
    }
}
pub fn do_special(p: &mut Player, r: &Rules, targets: &[Target]) -> Vec<Event> {
    if !r.playing || p.special_cd > 0.0 || p.dizzy > 0.0 || p.out || !r.has_player {
        return vec![];
    }
    p.special_cd = r.special_cooldown();
    if r.daddio {
        p.mega_t = 0.6;
        return vec![Event::Cast { key: "mega".into() }];
    }
    let rs = r.rider_skill;
    let mut out = vec![];
    match r.path {
        Path::Shadow => {
            let t = if rs >= 1 { 3.2_f64 } else { 2.2 };
            p.shadow = t;
            p.safe = p.safe.max(t);
        }
        Path::Storm => {
            let mut live: Vec<_> = targets.iter().filter(|e| !e.dead && !e.calm).collect();
            live.sort_by(|a, b| {
                (a.x - p.x)
                    .hypot(a.y - p.y)
                    .total_cmp(&(b.x - p.x).hypot(b.y - p.y))
            });
            for e in live.into_iter().take(if rs >= 2 { 6 } else { 4 }) {
                out.push(hit(
                    e.id,
                    if rs >= 1 { 4.0 } else { 3.0 } * r.bellows_mult(),
                    if rs >= 1 { 2.5 } else { 2.0 },
                    false,
                    false,
                    false,
                ));
            }
            gems(&mut out, rs);
            p.safe = p.safe.max(0.6);
        }
        Path::Guardian => {
            p.shield = p.shield.max(if rs >= 1 { 4.5 } else { 3.0 });
            p.hearts = r.max_hearts.min(p.hearts.saturating_add(1));
            p.safe = p.safe.max(0.8);
            out = area(
                p,
                r,
                targets,
                if rs >= 2 { 4.2 } else { 3.2 },
                if rs >= 2 { 2.2 } else { 1.5 },
                false,
                false,
            );
            gems(&mut out, rs);
            out.push(Event::Cast {
                key: "guard".into(),
            });
        }
        Path::Forge => {
            out = area(
                p,
                r,
                targets,
                if rs >= 2 { 4.4 } else { 3.4 },
                if rs >= 1 { 3.5 } else { 2.5 },
                false,
                true,
            );
            p.safe = p.safe.max(0.8);
            gems(&mut out, rs);
        }
    }
    out
}
pub fn shadow_burst(p: &Player, r: &Rules, targets: &[Target]) -> Vec<Event> {
    let rs = r.rider_skill;
    let mut out = area(
        p,
        r,
        targets,
        if rs >= 2 { 4.2 } else { 3.2 },
        if rs >= 2 { 3.2 } else { 2.5 },
        true,
        false,
    );
    gems(&mut out, rs);
    out
}
pub fn mega_blast(targets: &[Target]) -> Vec<Event> {
    targets
        .iter()
        .filter(|e| !e.dead && !e.calm)
        .map(|e| hit(e.id, 0.0, 0.0, false, false, true))
        .collect()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DadPower {
    Said,
    Joke,
}
impl DadPower {
    pub fn key(self) -> &'static str {
        match self {
            Self::Said => "said",
            Self::Joke => "joke",
        }
    }
    pub fn cooldown(self) -> f64 {
        match self {
            Self::Said => 45.0,
            Self::Joke => 30.0,
        }
    }
}
pub fn dad_power(
    p: &mut Player,
    r: &Rules,
    targets: &[Target],
    power: DadPower,
    mut random: impl FnMut() -> f64,
) -> Vec<Event> {
    if !r.playing
        || !r.daddio
        || p.dizzy > 0.0
        || p.down > 0.0
        || p.out
        || p.dad_cd.get(power.key()).copied().unwrap_or(0.0) > 0.0
    {
        return vec![];
    }
    p.dad_cd.insert(power.key().into(), power.cooldown());
    let mut out: Vec<_> = targets
        .iter()
        .filter(|e| !e.calm && !e.dead)
        .map(|e| {
            if power == DadPower::Said && !e.boss {
                Event::Calm { id: e.id }
            } else {
                hit(
                    e.id,
                    0.0,
                    if power == DadPower::Said { 4.0 } else { 3.0 },
                    false,
                    false,
                    false,
                )
            }
        })
        .collect();
    let key = if power == DadPower::Said {
        "said".into()
    } else {
        format!("joke:{}", (random() * 9.0).floor() as usize)
    };
    out.push(Event::Cast { key });
    out
}
pub fn cast_spell(p: &mut Player, r: &Rules, targets: &[Target], spell: Spell) -> Vec<Event> {
    if !r.playing || p.dizzy > 0.0 || p.out || !r.active_words.contains(&spell) {
        return vec![];
    }
    let cost = spell.cost(r.magic_skill);
    if p.energy < cost {
        return vec![Event::TooTired];
    }
    p.energy -= cost;
    let mut out = apply_spell(p, r, targets, spell);
    if spell.team() {
        out.push(Event::Cast {
            key: spell.key().into(),
        });
    }
    out
}
/// Ungated applySpell, also usable by the network adapter for team spells.
pub fn apply_spell(p: &mut Player, r: &Rules, targets: &[Target], spell: Spell) -> Vec<Event> {
    let sp = spell_power(r.magic_skill);
    let mut out = vec![];
    match spell {
        Spell::Naina => out.push(Event::GemRing {
            count: (if r.magic_skill >= 3 { 9 } else { 6 }) + if sp > 1.0 { 3 } else { 0 },
            rich: false,
        }),
        Spell::Kaelo => p.magnet = 4.0 * sp,
        Spell::Vara => {
            for e in targets {
                if !e.boss && (e.x - p.x).hypot(e.y - p.y) < r.scale * 4.5 * sp {
                    out.push(Event::Calm { id: e.id });
                }
            }
        }
        Spell::Sila => p.sila = 5.0 * sp,
        Spell::Thuri => p.shield = 6.0 * sp,
        Spell::Mira => out.push(Event::GemRing {
            count: if sp > 1.0 { 3 } else { 2 },
            rich: true,
        }),
        Spell::Senn => {
            out.push(Event::Rain { duration: 2.0 });
            out.extend(
                targets
                    .iter()
                    .map(|e| hit(e.id, 0.8 * sp, 0.0, false, false, false)),
            );
        }
        Spell::Faro => {
            out.extend(
                targets
                    .iter()
                    .map(|e| hit(e.id, 3.0 * sp * r.bellows_mult(), 0.0, false, false, false)),
            );
            p.tired = 6.0;
        }
    }
    out
}
