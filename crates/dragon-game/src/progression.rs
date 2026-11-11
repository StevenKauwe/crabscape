//! In-memory progression commands over normalized profiles, never browser storage.
//! Craft commands represent `doCraft` after any multiplayer vote has passed.
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TeamSummary {
    pub craft: Value,
    pub hoard: f64,
    pub treats_left: f64,
    pub hatched: f64,
    pub xp_multiplier: f64,
}
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
pub fn team_summary(me: &Value, others: &[Value], static_mode: bool) -> TeamSummary {
    let docs =
        std::iter::once(me).chain(
            others
                .iter()
                .take(if static_mode { 0 } else { others.len() }),
        );
    let mut craft = json!({"treatsMade":0,"tea":0,"tonic":0,"saddles":false,"armor":0,"bellows":0});
    let mut hoard = 0.0;
    let mut hatched = 0.0;
    for d in docs {
        hoard = hoard + n(&d["earned"]) - n(&d["spent"]);
        hatched += n(&d["hatched"]);
        for k in ["treatsMade", "tea", "tonic", "armor", "bellows"] {
            craft[k] = json!(n(&craft[k]) + n(&d["craft"][k]));
        }
        craft["saddles"] = json!(craft["saddles"] == true || d["craft"]["saddles"] == true);
    }
    for k in ["tea", "tonic", "armor", "bellows"] {
        craft[k] = json!(n(&craft[k]).min(3.0));
    }
    TeamSummary {
        hoard: hoard.floor().max(0.0),
        treats_left: (n(&craft["treatsMade"]) - n(&me["treatsUsed"])).max(0.0),
        hatched,
        xp_multiplier: (1.0 + 0.25 * n(&craft["tonic"])) * if hatched >= 16.0 { 1.25 } else { 1.0 },
        craft,
    }
}
pub fn stars_left(me: &Value) -> f64 {
    (n(&me["stars"])
        - n(&me["skills"]["breath"])
        - n(&me["skills"]["rider"])
        - n(&me["skills"]["magic"]))
    .max(0.0)
}
pub fn known_words(me: &Value) -> Vec<&'static str> {
    [
        ("naina", ""),
        ("thuri", "m2"),
        ("kaelo", "m3"),
        ("vara", "m5"),
        ("sila", "m6"),
        ("mira", "m7"),
        ("senn", "m9"),
        ("faro", "m11"),
    ]
    .into_iter()
    .filter(|(_, m)| {
        m.is_empty()
            || me["cleared"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == m))
    })
    .map(|(w, _)| w)
    .collect()
}
pub fn riding(me: &Value, team: &TeamSummary) -> bool {
    crate::level_of(n(&me["party"][0]["xp"])) >= 8 && team.craft["saddles"] == true
}
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    AddXp { dragon: String, amount: f64 },
    Feed(String),
    Treat(String),
    Spend { cost: f64, text: String },
    Craft(String),
    Build(String),
    LearnSkill(String),
    SelectMain(String),
    Swap,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Growth {
        dragon: String,
        level_before: u8,
        level_after: u8,
        stage_before: u8,
        stage_after: u8,
        stars: f64,
        riding_unlocked: bool,
    },
    Spent {
        cost: f64,
    },
    Crafted {
        item: String,
    },
    Built {
        project: String,
        thanks: String,
        spot: [f64; 2],
    },
    Learned {
        branch: String,
        name: String,
    },
    Selected {
        dragon: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidProfile,
    InvalidNumber,
    UnknownDragon,
    UnknownItem,
    UnknownSkill,
    Complete,
    NotEnoughGems,
    NoTreats,
    NoStars,
    NeedsWords,
    LockedProject,
}
fn stage(level: u8) -> u8 {
    if level >= 12 {
        3
    } else if level >= 8 {
        2
    } else if level >= 4 {
        1
    } else {
        0
    }
}
fn dragon_index(me: &Value, id: &str) -> Result<usize, Error> {
    me["party"]
        .as_array()
        .ok_or(Error::InvalidProfile)?
        .iter()
        .position(|d| d["id"] == id)
        .ok_or(Error::UnknownDragon)
}
fn spend(
    me: &mut Value,
    team: &TeamSummary,
    cost: f64,
    text: &str,
    now: f64,
    events: &mut Vec<Event>,
) -> Result<(), Error> {
    if !cost.is_finite() || cost < 0.0 {
        return Err(Error::InvalidNumber);
    }
    if team.hoard < cost {
        return Err(Error::NotEnoughGems);
    }
    me["spent"] = json!(n(&me["spent"]) + cost);
    me["last"] = json!({"text":text,"at":now});
    events.push(Event::Spent { cost });
    Ok(())
}
fn add_xp(me: &mut Value, team: &TeamSummary, index: usize, amount: f64) -> Event {
    let was_riding = riding(me, team);
    let d = &mut me["party"][index];
    let before = crate::level_of(n(&d["xp"]));
    d["xp"] = json!((n(&d["xp"]) + amount).min(14440.0));
    let after = crate::level_of(n(&d["xp"]));
    let stars = if d["bonded"] == true {
        f64::from(after.saturating_sub(before))
    } else {
        0.0
    };
    let dragon = d["id"].as_str().unwrap_or("").to_owned();
    me["stars"] = json!(n(&me["stars"]) + stars);
    Event::Growth {
        dragon,
        level_before: before,
        level_after: after,
        stage_before: stage(before),
        stage_after: stage(after),
        stars,
        riding_unlocked: !was_riding && riding(me, team),
    }
}
fn skill_name(branch: &str, path: &str, lv: usize) -> Option<&'static str> {
    let names = match branch {
        "breath" => [
            "Hot Breath",
            "Big Fireballs",
            "Quick Breath",
            "Team Fire",
            "Triple Breath",
        ],
        "magic" => [
            "Deep Magic",
            "Strong Words",
            "Bright Light",
            "Big Magic",
            "Old Tongue Master",
        ],
        "rider" => match path {
            "shadow" => [
                "Long Shadow",
                "Shadow Burst",
                "Swift Shadow",
                "Brave Heart",
                "Shadow Gems",
            ],
            "forge" => [
                "Mighty Smash",
                "Big Shockwave",
                "Quick Forge",
                "Brave Heart",
                "Gem Quake",
            ],
            "storm" => [
                "Loud Thunder",
                "Chain Lightning",
                "Quick Storm",
                "Brave Heart",
                "Lucky Lightning",
            ],
            "guardian" => [
                "Long Light",
                "Big Glow",
                "Quick Light",
                "Brave Heart",
                "Golden Glow",
            ],
            _ => return None,
        },
        _ => return None,
    };
    names.get(lv).copied()
}
/// Call only with normalized profiles. `now_ms` is recorded in last-action data;
/// all returned events are presentation requests, not side effects.
pub fn apply(
    me: &mut Value,
    others: &[Value],
    static_mode: bool,
    command: Command,
    now_ms: f64,
) -> Result<Vec<Event>, Error> {
    if !me.is_object()
        || !me["party"].is_array()
        || me["party"].as_array().is_some_and(Vec::is_empty)
        || !me["craft"].is_object()
        || !me["skills"].is_object()
    {
        return Err(Error::InvalidProfile);
    }
    if !now_ms.is_finite() {
        return Err(Error::InvalidNumber);
    }
    let team = team_summary(me, others, static_mode);
    let mut events = Vec::new();
    match command {
        Command::Spend { cost, text } => spend(me, &team, cost, &text, now_ms, &mut events)?,
        Command::AddXp { dragon, amount } => {
            if !amount.is_finite() {
                return Err(Error::InvalidNumber);
            }
            let i = dragon_index(me, &dragon)?;
            events.push(add_xp(me, &team, i, amount));
        }
        cmd @ (Command::Feed(_) | Command::Treat(_)) => {
            let treat = matches!(cmd, Command::Treat(_));
            let id = match cmd {
                Command::Feed(id) | Command::Treat(id) => id,
                _ => unreachable!(),
            };
            let i = dragon_index(me, &id)?;
            let name = me["party"][i]["name"]
                .as_str()
                .ok_or(Error::InvalidProfile)?
                .to_owned();
            if treat {
                if team.treats_left <= 0.0 {
                    return Err(Error::NoTreats);
                }
                me["treatsUsed"] = json!(n(&me["treatsUsed"]) + 1.0);
                me["last"] = json!({"text":format!("gave {name} a Sunleaf Treat"),"at":now_ms});
            } else {
                spend(me, &team, 10.0, &format!("fed {name}"), now_ms, &mut events)?;
            }
            events.push(add_xp(
                me,
                &team,
                i,
                if treat { 150.0 } else { 20.0 } * team.xp_multiplier,
            ));
        }
        Command::Craft(id) => {
            let (name, herbs, base, max, once) = match id.as_str() {
                "treatsMade" => ("Sunleaf Treats", true, 20.0, false, false),
                "tea" => ("Swiftwing Tea", true, 0.0, true, false),
                "tonic" => ("Starbloom Tonic", true, 0.0, true, false),
                "saddles" => ("Riding Saddles", false, 60.0, false, true),
                "armor" => ("Scale Armor", false, 0.0, true, false),
                "bellows" => ("Fire Bellows", false, 0.0, true, false),
                _ => return Err(Error::UnknownItem),
            };
            let lvl = n(&team.craft[&id]);
            if (once && team.craft[&id] == true) || (max && lvl >= 3.0) {
                return Err(Error::Complete);
            }
            let cost = if max {
                [30.0, 60.0, 100.0][lvl as usize]
            } else {
                base
            };
            let verb = if herbs { "brewed" } else { "forged" };
            spend(
                me,
                &team,
                cost,
                &format!("{verb} {name}"),
                now_ms,
                &mut events,
            )?;
            me["craft"][&id] = if once {
                json!(true)
            } else {
                json!(n(&me["craft"][&id]) + 1.0)
            };
            events.push(Event::Crafted { item: id });
        }
        Command::Build(id) => {
            let project = crate::content::catalog()["projects"]
                .as_array()
                .and_then(|a| a.iter().find(|p| p["id"] == id))
                .ok_or(Error::UnknownItem)?;
            let docs: Vec<_> = std::iter::once(&*me)
                .chain(
                    others
                        .iter()
                        .take(if static_mode { 0 } else { others.len() }),
                )
                .collect();
            if docs.iter().any(|p| {
                p["built"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == &id))
            }) {
                return Err(Error::Complete);
            }
            if !docs.iter().any(|p| {
                p["cleared"]
                    .as_array()
                    .is_some_and(|a| a.contains(&project["at"]))
            }) {
                return Err(Error::LockedProject);
            }
            let name = project["name"].as_str().ok_or(Error::InvalidProfile)?;
            let mut chars = name.chars();
            let text = chars
                .next()
                .map(|c| c.to_lowercase().collect::<String>() + chars.as_str())
                .unwrap_or_default();
            spend(me, &team, n(&project["cost"]), &text, now_ms, &mut events)?;
            me["built"]
                .as_array_mut()
                .ok_or(Error::InvalidProfile)?
                .push(json!(id));
            events.push(Event::Built {
                project: id,
                thanks: project["thanks"].as_str().unwrap_or("").into(),
                spot: [n(&project["spot"][0]), n(&project["spot"][1])],
            });
        }
        Command::LearnSkill(branch) => {
            if !["breath", "rider", "magic"].contains(&branch.as_str()) {
                return Err(Error::UnknownSkill);
            }
            let lv = n(&me["skills"][&branch]) as usize;
            if lv >= 5 {
                return Err(Error::Complete);
            }
            if branch == "magic" && lv == 4 && known_words(me).len() < 3 {
                return Err(Error::NeedsWords);
            }
            if stars_left(me) < 1.0 {
                return Err(Error::NoStars);
            }
            let path = me["build"]["path"].as_str().unwrap_or("shadow");
            let name = skill_name(&branch, path, lv).ok_or(Error::UnknownSkill)?;
            me["skills"][&branch] = json!(lv + 1);
            me["last"] = json!({"text":format!("learned {name}"),"at":now_ms});
            events.push(Event::Learned {
                branch,
                name: name.into(),
            });
        }
        Command::SelectMain(id) => {
            dragon_index(me, &id)?;
            me["main"] = json!(id);
            events.push(Event::Selected { dragon: id });
        }
        Command::Swap => {
            let party = me["party"].as_array().ok_or(Error::InvalidProfile)?;
            if party.len() < 2 {
                return Ok(events);
            }
            let index = party
                .iter()
                .position(|d| d["id"] == me["main"])
                .unwrap_or(0);
            let id = party[(index + 1) % party.len()]["id"]
                .as_str()
                .ok_or(Error::InvalidProfile)?
                .to_owned();
            me["main"] = json!(id);
            events.push(Event::Selected { dragon: id });
        }
    }
    Ok(events)
}
