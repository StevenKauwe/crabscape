//! Egg and valley state changes on caller-owned normalized profiles only.
//! Save events are intents, never permission to replace an original browser save.
use crate::profile::{ProfileError, clean_name};
use serde::Serialize;
use serde_json::{Value, json};

const EGG_KINDS: [&str; 4] = ["flame", "spark", "boulder", "frost"];
const NAMES: [&str; 22] = [
    "Sparky",
    "Ember",
    "Pickles",
    "Noodle",
    "Biscuit",
    "Comet",
    "Mango",
    "Pebble",
    "Waffles",
    "Ziggy",
    "Blaze",
    "Sprinkles",
    "Nugget",
    "Twinkle",
    "Fern",
    "Cinder",
    "Pip",
    "Thistle",
    "Maple",
    "Juniper",
    "Rocket",
    "Clover",
];
const MILESTONES: [(&str, f64); 7] = [
    ("first", 1.0),
    ("song", 3.0),
    ("gifts", 5.0),
    ("wing", 8.0),
    ("golden", 12.0),
    ("festival", 16.0),
    ("skies", 20.0),
];
fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
pub fn egg_ready(me: &Value) -> bool {
    me.is_object() && num(&me["eggs"]) > 0.0 && me["eggAwake"] == true
}
pub fn team_slots(me: &Value) -> usize {
    let n = me["cleared"].as_array().map_or(0, Vec::len);
    2 + usize::from(n >= 3) + usize::from(n >= 5) + usize::from(n >= 8)
}
fn docs<'a>(
    me: &'a Value,
    others: &'a [Value],
    static_mode: bool,
) -> impl Iterator<Item = &'a Value> {
    std::iter::once(me).chain(
        others
            .iter()
            .take(if static_mode { 0 } else { others.len() }),
    )
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Consume these draws in order when constructing the source confetti particles.
    Confetti {
        count: usize,
        draws: Vec<f64>,
    },
    Sound {
        name: String,
    },
    ShowHatch,
    Toast {
        text: String,
    },
    ForgetFollower {
        dragon: String,
    },
    Dirty,
    Flush,
    Blur,
    ToRoost,
    ShowMilestone {
        id: String,
    },
    HideMilestone,
    RenderRoost,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidProfile,
    InvalidRandom,
    NoBaby,
    InvalidRelease,
    Name(ProfileError),
}
impl From<ProfileError> for Error {
    fn from(e: ProfileError) -> Self {
        Self::Name(e)
    }
}
#[derive(Clone, Debug, Default)]
pub struct Roost {
    pub pending_baby: Option<Value>,
    pub suggested_name: String,
    pub milestone_queue: Vec<String>,
}
fn draw(rng: &mut impl FnMut() -> f64) -> Result<f64, Error> {
    let x = rng();
    if x.is_finite() && (0.0..1.0).contains(&x) {
        Ok(x)
    } else {
        Err(Error::InvalidRandom)
    }
}
fn base36(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut s = Vec::new();
    while n > 0 {
        s.push(char::from_digit((n % 36) as u32, 36).expect("base36"));
        n /= 36;
    }
    s.into_iter().rev().collect()
}
fn color(kind: &str, amount: f64) -> String {
    let rgb = match kind {
        "flame" => [255.0, 106.0, 61.0],
        "spark" => [232.0, 185.0, 35.0],
        "boulder" => [154.0, 115.0, 80.0],
        "frost" => [108.0, 198.0, 240.0],
        _ => [232.0, 194.0, 90.0],
    };
    let target = if amount < 0.0 { 0.0 } else { 255.0 };
    let p = amount.abs();
    format!(
        "#{:02x}{:02x}{:02x}",
        ((target - rgb[0]) * p + rgb[0]).round() as u8,
        ((target - rgb[1]) * p + rgb[1]).round() as u8,
        ((target - rgb[2]) * p + rgb[2]).round() as u8
    )
}
fn validate(me: &Value) -> Result<(), Error> {
    if !me["party"].is_array()
        || !me["wild"].is_array()
        || !me["kindsSeen"].is_array()
        || !me["seenMs"].is_array()
    {
        Err(Error::InvalidProfile)
    } else {
        Ok(())
    }
}
fn release(
    me: &mut Value,
    dragon: Value,
    party_index: Option<usize>,
    rider_index: usize,
    now_ms: u64,
    quiet: bool,
) -> Vec<Event> {
    if let Some(index) = party_index {
        me["party"].as_array_mut().expect("validated").remove(index);
    }
    if me["main"] == dragon["id"] {
        me["main"] = json!("bond");
    }
    let id = dragon["id"].as_str().unwrap_or("").to_owned();
    let name = dragon["name"].as_str().unwrap_or("");
    me["released"] = json!(num(&me["released"]) + 1.0);
    let record = json!({"name":dragon["name"],"kind":dragon["kind"],"color":dragon["color"],"realm":me["lastRealm"],"from":rider_index});
    let wild = me["wild"].as_array_mut().expect("validated");
    wild.push(record);
    let excess = wild.len().saturating_sub(40);
    wild.drain(..excess);
    me["last"] = json!({"text":format!("sent {name} to the Dragon Valley"),"at":now_ms});
    let mut events = vec![
        Event::ForgetFollower { dragon: id },
        Event::Toast {
            text: format!("{name} flew to the Dragon Valley to live free. Visit at the Roost!"),
        },
    ];
    if !quiet {
        events.extend([Event::Dirty, Event::Flush]);
    }
    events
}
impl Roost {
    /// Source `hatch`, called after the times-table gate. Does not consume an egg
    /// until finish_hatch. Date and every gameplay/presentation RNG draw are explicit.
    pub fn begin_hatch(
        &mut self,
        me: &Value,
        others: &[Value],
        static_mode: bool,
        now_ms: u64,
        reduce_motion: bool,
        mut rng: impl FnMut() -> f64,
    ) -> Result<Vec<Event>, Error> {
        validate(me)?;
        let kind = if me["goldenPending"] == true {
            "star"
        } else {
            let unseen: Vec<_> = EGG_KINDS
                .into_iter()
                .filter(|k| {
                    !me["kindsSeen"]
                        .as_array()
                        .expect("validated")
                        .iter()
                        .any(|v| v == k)
                })
                .collect();
            let choices = if unseen.is_empty() {
                &EGG_KINDS[..]
            } else {
                &unseen
            };
            choices[(draw(&mut rng)? * choices.len() as f64).floor() as usize]
        };
        let mut used = Vec::new();
        for d in docs(me, others, static_mode) {
            for key in ["party", "wild"] {
                if let Some(a) = d[key].as_array() {
                    for dragon in a {
                        if let Some(name) = dragon["name"].as_str() {
                            used.push(name);
                        }
                    }
                }
            }
        }
        let free: Vec<_> = NAMES.into_iter().filter(|n| !used.contains(n)).collect();
        let id = format!(
            "d{}{}",
            base36(now_ms),
            base36((draw(&mut rng)? * 1e4).floor() as u64)
        );
        let baby = json!({"id":id,"name":"","kind":kind,"color":color(kind,-0.12+draw(&mut rng)?*0.24),"xp":0,"bonded":false});
        let count = if reduce_motion { 40 } else { 150 };
        let mut draws = Vec::with_capacity(count * 8);
        for _ in 0..count * 8 {
            draws.push(draw(&mut rng)?);
        }
        let choices = if free.is_empty() { &NAMES[..] } else { &free };
        let suggested =
            choices[(draw(&mut rng)? * choices.len() as f64).floor() as usize].to_owned();
        self.pending_baby = Some(baby);
        self.suggested_name = suggested;
        Ok(vec![
            Event::Confetti { count, draws },
            Event::Sound {
                name: "hatch".into(),
            },
            Event::ShowHatch,
        ])
    }
    /// `release_id` must be a selectable nonbonded dragon or the pending baby when
    /// full. Invalid selections are rejected before mutation (the UI disables finish).
    pub fn finish_hatch(
        &mut self,
        me: &mut Value,
        name: &str,
        release_id: Option<&str>,
        rider_index: usize,
        now_ms: u64,
    ) -> Result<Vec<Event>, Error> {
        validate(me)?;
        let mut baby = self.pending_baby.clone().ok_or(Error::NoBaby)?;
        let cleaned = clean_name(&json!(name))?;
        baby["name"] = json!(if cleaned.is_empty() {
            "Sparky"
        } else {
            &cleaned
        });
        let full = me["party"].as_array().expect("validated").len() >= team_slots(me);
        let baby_chosen = release_id.is_some_and(|id| baby["id"] == id);
        let old_index = release_id.and_then(|id| {
            me["party"]
                .as_array()
                .expect("validated")
                .iter()
                .position(|d| d["id"] == id && d["bonded"] != true)
        });
        if full && !baby_chosen && old_index.is_none() {
            return Err(Error::InvalidRelease);
        }
        self.pending_baby = None;
        if baby["kind"] != "star" {
            let seen = me["kindsSeen"].as_array_mut().expect("validated");
            seen.push(baby["kind"].clone());
            let mut unique = Vec::new();
            for v in seen.drain(..) {
                if !unique.contains(&v) {
                    unique.push(v);
                }
            }
            *seen = unique;
        }
        me["goldenPending"] = json!(false);
        me["hatched"] = json!(num(&me["hatched"]) + 1.0);
        me["eggs"] = json!((num(&me["eggs"]) - 1.0).max(0.0));
        let mut events = Vec::new();
        if full {
            if baby_chosen {
                events.extend(release(me, baby.clone(), None, rider_index, now_ms, true));
            } else if let Some(index) = old_index {
                let old = me["party"][index].clone();
                events.extend(release(me, old, Some(index), rider_index, now_ms, true));
                me["party"]
                    .as_array_mut()
                    .expect("validated")
                    .push(baby.clone());
            }
        } else {
            me["party"]
                .as_array_mut()
                .expect("validated")
                .push(baby.clone());
            let label = match baby["kind"].as_str().unwrap_or("") {
                "flame" => "Flame Spread",
                "spark" => "Spark Shot",
                "boulder" => "Boulder Blast",
                "frost" => "Frost Breath",
                _ => "Star Breath",
            };
            events.push(Event::Toast {
                text: format!(
                    "{} joined your team! Tap {} in flight to use {label}.",
                    baby["name"].as_str().unwrap_or(""),
                    baby["name"].as_str().unwrap_or("")
                ),
            });
        }
        me["last"] =
            json!({"text":format!("hatched {}",baby["name"].as_str().unwrap_or("")),"at":now_ms});
        events.extend([Event::Dirty, Event::Flush, Event::Blur, Event::ToRoost]);
        Ok(events)
    }
    /// Release by party index, preserving source object identity even with duplicate IDs.
    pub fn set_free(
        &self,
        me: &mut Value,
        index: usize,
        rider_index: usize,
        now_ms: u64,
        quiet: bool,
    ) -> Result<Vec<Event>, Error> {
        validate(me)?;
        let d = me["party"]
            .get(index)
            .filter(|d| d["bonded"] != true)
            .cloned()
            .ok_or(Error::InvalidRelease)?;
        Ok(release(me, d, Some(index), rider_index, now_ms, quiet))
    }
    pub fn check_milestones(
        &mut self,
        me: &mut Value,
        others: &[Value],
        static_mode: bool,
        show: bool,
        screen_hidden: bool,
    ) -> Result<Vec<Event>, Error> {
        validate(me)?;
        let hatched = crate::progression::team_summary(me, others, static_mode).hatched;
        let mut events = Vec::new();
        if hatched >= 12.0 && me["goldenGiven"] != true {
            me["goldenGiven"] = json!(true);
            me["goldenPending"] = json!(true);
            me["eggs"] = json!(num(&me["eggs"]) + 1.0);
            me["eggAwake"] = json!(true);
            events.push(Event::Dirty);
        }
        for (id, n) in MILESTONES {
            if hatched >= n
                && !me["seenMs"]
                    .as_array()
                    .expect("validated")
                    .iter()
                    .any(|v| v == id)
                && !self.milestone_queue.iter().any(|v| v == id)
            {
                self.milestone_queue.push(id.into());
            }
        }
        if show
            && screen_hidden
            && let Some(id) = self.milestone_queue.first()
        {
            events.push(Event::ShowMilestone { id: id.clone() });
        }
        Ok(events)
    }
    pub fn close_milestone(&mut self, me: &mut Value, in_roost: bool) -> Result<Vec<Event>, Error> {
        validate(me)?;
        let mut events = Vec::new();
        if !self.milestone_queue.is_empty() {
            let id = self.milestone_queue.remove(0);
            let seen = me["seenMs"].as_array_mut().expect("validated");
            if !seen.iter().any(|v| v == &id) {
                seen.push(json!(id));
                events.push(Event::Dirty);
            }
        }
        if let Some(id) = self.milestone_queue.first() {
            events.push(Event::ShowMilestone { id: id.clone() });
        } else {
            events.extend([Event::HideMilestone, Event::Blur]);
            if in_roost {
                events.push(Event::RenderRoost);
            }
        }
        Ok(events)
    }
}

/// Mission-end inputs from the simulation. `mission_id` is the base m1..m24 ID,
/// `par` is the effective champion timer, and `day` is the caller's local dayKey.
#[derive(Clone, Debug, Serialize)]
pub struct Completion {
    pub mission_id: String,
    pub champion: bool,
    pub ok: bool,
    pub carry: f64,
    pub banked: f64,
    pub bonks: u32,
    pub elapsed: f64,
    pub par: f64,
    pub had_objective: bool,
    pub day: i64,
    pub now_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CompletionEvent {
    Banked {
        gems: f64,
    },
    Daily {
        gems: f64,
    },
    TeamSlot {
        slots: usize,
    },
    Egg,
    BossStar,
    LearnedWord {
        word: String,
    },
    Rank {
        rank: usize,
    },
    Crowns {
        got: u8,
        new: u8,
        gems: f64,
    },
    ValleyGift {
        gems: f64,
    },
    ThankGift {
        gems: f64,
    },
    Dirty,
    Flush,
    /// The adapter shows mission story, then rank story if any, then results.
    Results {
        story: Option<String>,
        rank: Option<usize>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CompletionOutcome {
    pub carry: f64,
    pub banked: f64,
    pub first: bool,
    pub egg: bool,
    pub daily_reward: bool,
    pub crown_bits: Option<u8>,
    pub events: Vec<CompletionEvent>,
}
fn has(me: &Value, key: &str, id: &str) -> bool {
    me[key]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == id))
}
fn rank(me: &Value) -> usize {
    ["m4", "m8", "m12", "m16", "m20", "m24"]
        .iter()
        .enumerate()
        .filter(|(_, id)| has(me, "cleared", id))
        .map(|(i, _)| i + 1)
        .max()
        .unwrap_or(0)
}
fn equip_word(me: &mut Value, key: &str) -> bool {
    let known = crate::progression::known_words(me);
    let slots = if rank(me) >= 3 { 3 } else { 2 };
    let mut picked: Vec<String> = me["loadout"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|k| known.contains(k))
        .take(slots)
        .map(str::to_owned)
        .collect();
    if picked.is_empty() {
        picked = known.into_iter().take(slots).map(str::to_owned).collect();
    }
    if picked.iter().any(|k| k == key) {
        return false;
    }
    if picked.len() >= slots {
        picked.pop();
    }
    picked.push(key.into());
    me["loadout"] = json!(picked);
    true
}
/// Source endMission's persistent reward projection, plus banked/carry runtime
/// updates. Does not render result prose, mutate the mission, perform handoff,
/// or save anything. Calling twice represents two results; caller owns idempotence.
pub fn complete_mission(
    me: &mut Value,
    others: &[Value],
    static_mode: bool,
    input: &Completion,
) -> Result<CompletionOutcome, Error> {
    validate(me)?;
    if !me["cleared"].is_array()
        || !me["crowns"].is_object()
        || input.day < 0
        || ![input.carry, input.banked, input.elapsed, input.par]
            .iter()
            .all(|n| n.is_finite() && *n >= 0.0)
    {
        return Err(Error::InvalidProfile);
    }
    let definitions = crate::missions::missions();
    let def = definitions
        .iter()
        .find(|m| m.id == input.mission_id)
        .ok_or(Error::InvalidProfile)?;
    let number = definitions
        .iter()
        .position(|m| m.id == input.mission_id)
        .expect("known mission")
        + 1;
    let first = input.ok && !input.champion && !has(me, "cleared", &def.id);
    let mut out = CompletionOutcome {
        carry: input.carry,
        banked: input.banked,
        first,
        egg: false,
        daily_reward: false,
        crown_bits: None,
        events: Vec::new(),
    };
    if input.ok && input.carry > 0.0 {
        me["earned"] = json!(num(&me["earned"]) + input.carry);
        out.banked += input.carry;
        out.carry = 0.0;
        out.events.extend([
            CompletionEvent::Banked { gems: input.carry },
            CompletionEvent::Dirty,
        ]);
    }
    let daily_done =
        me["daily"]["day"].as_f64() == Some(input.day as f64) && me["daily"]["done"] == true;
    let daily_mission = ((input.day as f64 * 7.0) % 24.0) as usize + 1;
    let daily_ok = input.ok
        && match input.day % 5 {
            0 => number == daily_mission && input.bonks <= 2,
            1 => out.banked >= 40.0,
            2 => input.champion,
            3 => number == daily_mission && input.bonks == 0,
            4 => input.had_objective,
            _ => false,
        };
    if !daily_done && daily_ok {
        me["daily"] = json!({"day":input.day,"done":true});
        me["earned"] = json!(num(&me["earned"]) + 40.0);
        out.daily_reward = true;
        out.events.extend([
            CompletionEvent::Daily { gems: 40.0 },
            CompletionEvent::Dirty,
        ]);
    }
    let mut new_rank = None;
    if input.ok {
        if first {
            let before = team_slots(me);
            me["cleared"]
                .as_array_mut()
                .expect("validated")
                .push(json!(def.id));
            if team_slots(me) > before {
                out.events.push(CompletionEvent::TeamSlot {
                    slots: team_slots(me),
                });
            }
            if number % 2 == 0 {
                me["eggs"] = json!(num(&me["eggs"]) + 1.0);
                out.egg = true;
                out.events.push(CompletionEvent::Egg);
            }
            if def.boss.is_some() {
                me["stars"] = json!(num(&me["stars"]) + 1.0);
                out.events.push(CompletionEvent::BossStar);
            }
            for (word, at) in [
                ("thuri", 2),
                ("kaelo", 3),
                ("vara", 5),
                ("sila", 6),
                ("mira", 7),
                ("senn", 9),
                ("faro", 11),
            ] {
                if number == at {
                    if equip_word(me, word) {
                        out.events.push(CompletionEvent::Dirty);
                    }
                    out.events
                        .push(CompletionEvent::LearnedWord { word: word.into() });
                }
            }
            if number % 4 == 0 {
                let r = number / 4;
                new_rank = Some(r);
                out.events.push(CompletionEvent::Rank { rank: r });
            }
        } else {
            me["replays"] = json!(num(&me["replays"]) + 1.0);
            if num(&me["replays"]) % 3.0 == 0.0 {
                me["eggs"] = json!(num(&me["eggs"]) + 1.0);
                out.egg = true;
                out.events.push(CompletionEvent::Egg);
            }
        }
        if input.champion {
            let old = num(&me["crowns"][&def.id]) as u8;
            let got = 1
                | if input.bonks <= 2 { 2 } else { 0 }
                | if input.elapsed <= input.par { 4 } else { 0 };
            let new = (got & !old).count_ones() as u8;
            let gems = 15.0 * f64::from(new);
            me["crowns"][&def.id] = json!(old | got);
            me["earned"] = json!(num(&me["earned"]) + gems);
            out.crown_bits = Some(got);
            out.events.push(CompletionEvent::Crowns { got, new, gems });
            if (old | got) == 7 && old != 7 {
                me["eggs"] = json!(num(&me["eggs"]) + 1.0);
                out.egg = true;
                out.events.push(CompletionEvent::Egg);
            }
        }
        let team = crate::progression::team_summary(me, others, static_mode);
        let wild: usize = docs(me, others, static_mode)
            .map(|d| d["wild"].as_array().map_or(0, Vec::len))
            .sum();
        if team.hatched >= 5.0 && wild > 0 {
            let gems = (2 * wild).min(20) as f64;
            me["earned"] = json!(num(&me["earned"]) + gems);
            out.events.push(CompletionEvent::ValleyGift { gems });
        }
        let mut built = std::collections::BTreeSet::new();
        for d in docs(me, others, static_mode) {
            for v in d["built"].as_array().into_iter().flatten() {
                if let Some(id) = v.as_str() {
                    built.insert(id.to_owned());
                }
            }
        }
        let thanks = 2.0 * built.len() as f64;
        if thanks > 0.0 {
            me["earned"] = json!(num(&me["earned"]) + thanks);
            out.events.push(CompletionEvent::ThankGift { gems: thanks });
        }
        if num(&me["eggs"]) > 0.0 {
            me["eggAwake"] = json!(true);
        }
        me["last"] = json!({"text":format!("cleared {}",def.name),"at":input.now_ms});
    }
    out.events.extend([
        CompletionEvent::Dirty,
        CompletionEvent::Flush,
        CompletionEvent::Results {
            story: if first { Some(def.id.clone()) } else { None },
            rank: new_rank,
        },
    ]);
    Ok(out)
}
