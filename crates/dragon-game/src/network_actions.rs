//! Pure shared-sky actions, host takeover and gameplay presence projection.
//! Adapters supply admitted transport identities, never payload claims, and apply
//! returned effects in order. Transport sequence checks happen before these APIs.
//! Valid source packets retain their behavior. Hardenings: rider spoofing, unsafe
//! numeric IDs, unknown peers and malformed action fields are rejected. No database,
//! visual team customization, lobby votes, audio capture, timer, save or network IO.
use crate::{
    abilities::Spell,
    combat::{Kind as ShotKind, Shot},
    enemies::{Enemy, Kind},
    missions::Leg,
    network_world::{GuestWorld, Viewport},
};
use serde_json::Value;
use std::collections::BTreeMap;

/// Binding from an admitted transport peer's validated presence/slot assignment.
/// Do not construct this from the action payload's `r` field.
#[derive(Clone, Copy, Debug)]
pub struct Sender<'a> {
    pub peer: &'a str,
    pub rider: usize,
    pub daddio: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum HitAction {
    Calm,
    Hit(Shot),
}
#[derive(Clone, Debug, PartialEq)]
pub struct AdmittedHit {
    pub uid: u64,
    pub rider: usize,
    /// A calm request on a boss still counts as an admitted remote hit, but does nothing.
    pub action: Option<HitAction>,
}
/// Host-only admission. Invoke the normal hit/defeat path synchronously with this
/// attribution; it may create ordered defeat/gem/XP effects. No alternate damage rules.
pub fn remote_hit(
    world: &GuestWorld,
    enemies: &[Enemy],
    sender: Option<Sender<'_>>,
    data: &Value,
) -> Option<AdmittedHit> {
    let sender = sender?;
    if world.authority.guest || !same_run(world, data) || id(&data["r"])? != sender.rider as u64 {
        return None;
    }
    let uid = id(&data["uid"])?;
    let enemy = enemies
        .iter()
        .find(|e| e.target.id == uid && !e.target.dead && !e.target.calm)?;
    let action = if truthy(&data["calm"]) {
        (!enemy.target.boss).then_some(HitAction::Calm)
    } else {
        let kind = data["kind"]
            .as_str()
            .map(|s| s.chars().take(10).collect::<String>())
            .and_then(|s| serde_json::from_value::<ShotKind>(Value::String(s)).ok())
            .unwrap_or(ShotKind::Area);
        Some(HitAction::Hit(Shot {
            dmg: number(&data["dmg"]).clamp(0., 20.),
            kind,
            slow: number(&data["slow"]).clamp(0., 3.),
            stun: number(&data["stun"]).clamp(0., 5.),
            mega: one(&data["mega"]) && sender.daddio,
            mark: one(&data["mk"]),
            crack: one(&data["cr"]),
            owner: "remote".into(),
            ..Shot::default()
        }))
    };
    Some(AdmittedHit {
        uid,
        rider: sender.rider,
        action,
    })
}
#[derive(Clone, Debug, PartialEq)]
pub enum DefeatEffect {
    RemoveEnemy(u64),
    /// Route through the existing defeat presentation/gem and boss-reward path.
    /// Gems are local drops from this effect; there is no separate remote gem grant topic.
    Defeat {
        kind: Kind,
        position: [f64; 2],
        radius: f64,
        calmed: bool,
    },
    CreditLocalKill {
        xp: u32,
    },
}
/// A new authenticated host defeat event has source semantics even when its enemy
/// is already absent. Deduplicate packet sequence in transport, not by enemy UID.
pub fn remote_defeat(
    world: &GuestWorld,
    sender: Option<Sender<'_>>,
    data: &Value,
    local_rider: usize,
    view: Viewport,
) -> Option<Vec<DefeatEffect>> {
    let sender = sender?;
    if !world.authority.guest
        || world.authority.host_peer.as_deref() != Some(sender.peer)
        || !same_run(world, data)
        || id(&data["r"])? != sender.rider as u64
    {
        return None;
    }
    let uid = id(&data["uid"])?;
    let kind = serde_json::from_value::<Kind>(data["type"].clone()).ok()?;
    let def = kind.definition();
    let mut effects = vec![
        DefeatEffect::RemoveEnemy(uid),
        DefeatEffect::Defeat {
            kind,
            position: [
                number(&data["x"]).clamp(0., 1.) * view.width,
                view.top + number(&data["y"]).clamp(-0.2, 1.2) * (view.ground - view.top),
            ],
            radius: view.scale * def.radius,
            calmed: truthy(&data["calmed"]),
        },
    ];
    if id(&data["by"]) == Some(local_rider as u64) && !def.boss {
        effects.push(DefeatEffect::CreditLocalKill { xp: 4 });
    }
    Some(effects)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeoverReason {
    Lost,
    Handoff,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Takeover {
    pub previous_host_rider: usize,
    pub reason: TakeoverReason,
    pub enemies: Vec<Enemy>,
}
/// Restore the source host timers/queue/cooldowns. This consumes mirror metadata:
/// install the returned enemies into the host simulation before advertising authority.
/// The caller applies a takeover announcement; this function does not touch storage.
pub fn become_host(
    world: &mut GuestWorld,
    reason: TakeoverReason,
    draw: &mut impl FnMut() -> f64,
) -> Option<Takeover> {
    if !world.authority.guest || world.authority.epoch >= 9_007_199_254_740_991 {
        return None;
    }
    let previous_host_rider = world.authority.host_rider;
    world.authority.guest = false;
    world.authority.host_peer = None;
    world.authority.epoch += 1;
    let m = &mut world.mission;
    m.guest = false;
    m.queue = world.last_queue.iter().cloned().collect();
    m.spawn_t = 1.;
    if m.leg == Leg::Fly {
        m.leg_t = 4.;
        m.leg_max = 4.;
        m.leg_from = m.prog;
        m.trick_t = 2.;
    }
    let enemies = std::mem::take(&mut world.enemies)
        .into_iter()
        .map(|mirror| {
            let mut e = mirror.enemy;
            e.entered = true;
            e.cool = 1.5 + draw() * 1.5;
            e.cool2 = 4. + draw() * 3.;
            e.windup = 0.;
            if e.kind == Kind::Sprite {
                e.destination = None;
            }
            e
        })
        .collect::<Vec<_>>();
    if m.total == 0 {
        m.total = m.queue.len()
            + enemies
                .iter()
                .filter(|e| !e.extra && !e.target.calm)
                .count();
    }
    Some(Takeover {
        previous_host_rider,
        reason,
        enemies,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handoff {
    BecomeHost,
    Follow { rider: usize, peer: Option<String> },
}
/// Candidate order matches source activeOthers/Map insertion order. Unlike timeout
/// election, explicit handoff picks the lowest rider slot, not lexical peer ID.
pub fn handoff(
    world: &GuestWorld,
    sender: Option<Sender<'_>>,
    data: &Value,
    ghosts: &[Ghost],
    local_rider: usize,
    local_out: bool,
) -> Option<Handoff> {
    let sender = sender?;
    if !world.authority.guest
        || world.authority.host_peer.as_deref() != Some(sender.peer)
        || !same_run(world, data)
        || id(&data["r"])? != sender.rider as u64
    {
        return None;
    }
    let mut candidates = ghosts
        .iter()
        .filter(|g| {
            g.rider != local_rider
                && g.rider != sender.rider
                && g.mission.as_deref() == Some(&world.mission_id)
                && matches!(g.status.as_str(), "play" | "pause")
                && !g.out
        })
        .map(|g| g.rider)
        .collect::<Vec<_>>();
    if !local_out {
        candidates.push(local_rider);
    }
    match candidates.into_iter().min() {
        None => Some(Handoff::BecomeHost),
        Some(r) if r == local_rider => Some(Handoff::BecomeHost),
        Some(r) => Some(Handoff::Follow {
            rider: r,
            peer: ghosts.iter().find(|g| g.rider == r).map(|g| g.peer.clone()),
        }),
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Ghost {
    pub peer: String,
    pub rider: usize,
    pub status: String,
    pub target: [f64; 2],
    /// Existing renderer-interpolated position, not the incoming normalized target.
    pub position: Option<[f64; 2]>,
    pub facing: i8,
    pub riding: bool,
    pub shield: bool,
    pub out: bool,
    pub hearts: Option<f64>,
    pub mission: Option<String>,
    pub run: Option<String>,
    pub host: bool,
    pub epoch: u64,
    pub wave_kills: BTreeMap<i32, u32>,
    pub boss_damage: f64,
    pub cast_count: i32,
    pub was_frozen: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct PresenceContext<'a> {
    pub local_rider: usize,
    pub playing: bool,
    pub local_mission: Option<&'a str>,
    pub local_host: bool,
    pub objective_carrier: Option<usize>,
    pub player: [f64; 2],
    pub player_out: bool,
    pub scale: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PresenceEffect {
    DropPieceBonk,
    ApplyTeamSpell(Spell),
    AnnounceSpell(Spell),
    AnnounceJoke(usize),
    EpicJoke([f64; 2]),
    EpicMega([f64; 2]),
    /// Caller schedules a presentation-only boom after 0.6s, rechecking playing then.
    DelayedMegaBoom,
    AnnounceSaid,
    EpicSaid([f64; 2]),
    /// Caller heals min(max_hearts, hearts+1), raises shield to at least 2.5 and presents.
    GuardianHeal,
    AnnounceFrozen,
}
/// Live presence only. The caller validates/assigns the rider slot and supplies an
/// admitted Sender; mission validity is delegated to its existing definition lookup.
/// First presence seeds cast_count without replaying historical casts. Gameplay
/// effects deliberately preserve source team-spell (not same-mission) semantics.
pub fn ingest_presence(
    ghost: &mut Option<Ghost>,
    sender: Option<Sender<'_>>,
    data: &Value,
    ctx: PresenceContext<'_>,
    valid_mission: impl Fn(&str) -> bool,
) -> Option<Vec<PresenceEffect>> {
    let sender = sender?;
    if id(&data["r"])? != sender.rider as u64 {
        return None;
    }
    let x = data["x"].as_f64()?;
    let y = data["y"].as_f64()?;
    if !x.is_finite() || !y.is_finite() || ghost.as_ref().is_some_and(|g| g.peer != sender.peer) {
        return None;
    }
    let status = data["s"].as_str().unwrap_or("play").to_owned();
    let cs = data["cs"].as_array();
    let cast_count = cs.and_then(|a| a.first()).map_or(0, to_int32).max(0);
    let g = ghost.get_or_insert_with(|| Ghost {
        peer: sender.peer.into(),
        rider: sender.rider,
        status: status.clone(),
        target: [0., 0.],
        position: None,
        facing: 1,
        riding: false,
        shield: false,
        out: false,
        hearts: None,
        mission: None,
        run: None,
        host: false,
        epoch: 0,
        wave_kills: BTreeMap::new(),
        boss_damage: 0.,
        cast_count,
        was_frozen: status == "frozen",
    });
    g.rider = sender.rider;
    g.status = status;
    g.target = [x.clamp(0., 1.), y.clamp(0., 1.)];
    g.facing = if data["f"].as_f64() == Some(-1.) {
        -1
    } else {
        1
    };
    g.riding = one(&data["rd"]);
    g.shield = one(&data["sh"]);
    g.out = one(&data["o"]);
    let mut effects = Vec::new();
    if let Some(hearts) = data["h"].as_f64().filter(|h| h.is_finite()) {
        if g.hearts.is_some_and(|old| hearts < old)
            && ctx.local_host
            && ctx.local_mission.is_some()
            && ctx.objective_carrier == Some(g.rider)
        {
            effects.push(PresenceEffect::DropPieceBonk);
        }
        g.hearts = Some(hearts);
    }
    if let Some(ms) = data["ms"].as_array().filter(|a| {
        a.first()
            .and_then(Value::as_str)
            .is_some_and(&valid_mission)
    }) {
        g.host = ms.get(5).is_none_or(|v| v.as_f64() != Some(0.));
        g.epoch = number(get(ms, 6)).clamp(0., 1e9).floor() as u64;
        let run = ms.get(4).and_then(Value::as_str).map(str::to_owned);
        if g.run != run {
            g.run = run;
            g.wave_kills.clear();
            g.boss_damage = 0.;
        }
        g.mission = ms[0].as_str().map(str::to_owned);
        let kills = to_int32(get(ms, 2)).clamp(0, 99) as u32;
        let old = g.wave_kills.entry(to_int32(get(ms, 1))).or_default();
        *old = (*old).max(kills);
        g.boss_damage = g.boss_damage.max(number(get(ms, 3)).min(999.));
    } else {
        g.mission = None;
    }
    if g.rider != ctx.local_rider {
        if cast_count > g.cast_count {
            let cast = cs
                .and_then(|a| a.get(1))
                .and_then(Value::as_str)
                .unwrap_or("");
            let spell = Spell::ALL.into_iter().find(|s| s.key() == cast && s.team());
            let together = ctx.playing
                && g.mission.as_deref() == ctx.local_mission
                && ctx.local_mission.is_some();
            if let Some(spell) = spell.filter(|_| ctx.playing) {
                effects.push(PresenceEffect::ApplyTeamSpell(spell));
                effects.push(PresenceEffect::AnnounceSpell(spell));
            } else if let Some(joke) = cast
                .strip_prefix("joke:")
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| *n < 9)
            {
                effects.push(PresenceEffect::AnnounceJoke(joke));
                if let Some(p) = g.position.filter(|_| together) {
                    effects.push(PresenceEffect::EpicJoke(p));
                }
            } else if cast == "mega" {
                if let Some(p) = g.position.filter(|_| together) {
                    effects.push(PresenceEffect::EpicMega(p));
                    effects.push(PresenceEffect::DelayedMegaBoom);
                }
            } else if cast == "said" {
                effects.push(PresenceEffect::AnnounceSaid);
                if let Some(p) = g.position.filter(|_| together) {
                    effects.push(PresenceEffect::EpicSaid(p));
                }
            } else if cast == "guard"
                && !ctx.player_out
                && together
                && g.position.is_some_and(|p| {
                    (p[0] - ctx.player[0]).hypot(p[1] - ctx.player[1]) < ctx.scale * 5.5
                })
            {
                effects.push(PresenceEffect::GuardianHeal);
            }
        }
        if g.status == "frozen" && !g.was_frozen {
            effects.push(PresenceEffect::AnnounceFrozen);
        }
        g.was_frozen = g.status == "frozen";
    }
    g.cast_count = g.cast_count.max(cast_count);
    Some(effects)
}
fn same_run(w: &GuestWorld, d: &Value) -> bool {
    d.is_object()
        && d["mid"].as_str() == Some(&w.mission_id)
        && d["run"].as_str() == Some(&w.run)
        && id(&d["epoch"]) == Some(w.authority.epoch)
}
fn id(v: &Value) -> Option<u64> {
    let n = v.as_f64()?;
    ((0. ..=9_007_199_254_740_991.).contains(&n) && n.fract() == 0.).then_some(n as u64)
}
fn get(a: &[Value], i: usize) -> &Value {
    a.get(i).unwrap_or(&Value::Null)
}
fn number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| !n.is_nan())
            .unwrap_or(0.),
        Value::Bool(b) => u8::from(*b) as f64,
        _ => 0.,
    }
}
fn to_int32(v: &Value) -> i32 {
    let n = number(v);
    if !n.is_finite() {
        0
    } else {
        n.trunc().rem_euclid(4_294_967_296.) as u32 as i32
    }
}
fn one(v: &Value) -> bool {
    v.as_f64() == Some(1.)
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
