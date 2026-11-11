//! Source personal Roost actions; never persist or modify another rider's profile.
use super::*;

pub const CHARMS: [(&str, &str, f64, &str); 3] = [
    (
        "heart",
        "Lucky Heart",
        25.,
        "One extra heart for your next mission.",
    ),
    (
        "pouch",
        "Gem Pouch",
        30.,
        "Next mission, running out of hearts loses none of the gems you carry.",
    ),
    (
        "start",
        "Head Start",
        20.,
        "A shield and a gem magnet for the first ten seconds of your next mission.",
    ),
];
impl Adventure {
    pub fn leave_mission(&mut self) -> Result<Vec<Event>, Error> {
        if !matches!(self.state(), "play" | "pause") || self.game().is_none() {
            return Err(Error::InvalidState);
        }
        if let Some(session) = &mut self.shared {
            session.handoff();
        }
        self.to_map()
    }
    pub fn loadout_slots(&self) -> usize {
        let rank = ["m4", "m8", "m12", "m16", "m20", "m24"]
            .iter()
            .enumerate()
            .filter(|(_, id)| has(self.profile(), "cleared", id))
            .map(|(i, _)| i + 1)
            .max()
            .unwrap_or(0);
        crate::abilities::spell_slots(rank)
    }
    pub fn active_loadout(&self) -> Vec<String> {
        let known = progression::known_words(self.profile());
        let selected: Vec<_> = self.profile()["loadout"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|k| known.contains(k))
            .take(self.loadout_slots())
            .map(str::to_owned)
            .collect();
        if selected.is_empty() {
            known
                .into_iter()
                .take(self.loadout_slots())
                .map(str::to_owned)
                .collect()
        } else {
            selected
        }
    }
    fn require_roost(&self) -> Result<(), Error> {
        if matches!(self.state(), "roost" | "skills") {
            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }
    pub fn buy_charm(&mut self, id: &str, clock: Clock) -> Result<Vec<Event>, Error> {
        self.require_roost()?;
        let (_, name, cost, _) = CHARMS
            .iter()
            .find(|c| c.0 == id)
            .ok_or(Error::Runtime("Unknown charm"))?;
        let count = self.profile["charms"][id].as_f64().unwrap_or(0.) as u64;
        if count >= 3 {
            return Err(Error::Runtime("You already have three of this charm"));
        }
        let mut events = self.roost_command(
            progression::Command::Spend {
                cost: *cost,
                text: format!("bought a {name}"),
            },
            clock,
        )?;
        self.profile["charms"][id] = json!(count + 1);
        events.extend([
            Event::Sound {
                name: "gold".into(),
            },
            Event::Roost {
                value: roost::Event::Toast {
                    text: format!("{name}! It's ready for your next mission."),
                },
            },
        ]);
        Ok(events)
    }
    pub fn toggle_word(&mut self, word: &str) -> Result<Vec<Event>, Error> {
        self.require_roost()?;
        if !progression::known_words(self.profile()).contains(&word) {
            return Err(Error::Runtime("Unknown word"));
        }
        let mut selected = self.active_loadout();
        if let Some(index) = selected.iter().position(|w| w == word) {
            if selected.len() == 1 {
                return Err(Error::Runtime("Keep at least one word."));
            }
            selected.remove(index);
        } else {
            if selected.len() >= self.loadout_slots() {
                return Err(Error::Runtime("Tap a word to put it back first."));
            }
            selected.push(word.to_owned());
        }
        self.profile["loadout"] = json!(selected);
        Ok(vec![
            Event::Dirty,
            Event::Sound {
                name: "pick".into(),
            },
            Event::UpdateHud,
        ])
    }
    pub fn release_dragon(&mut self, index: usize, clock: Clock) -> Result<Vec<Event>, Error> {
        self.require_roost()?;
        let values = self.roost.set_free(
            &mut self.profile,
            index,
            self.gate.rider_index,
            clock.now_ms,
            false,
        )?;
        Ok(values
            .into_iter()
            .map(|value| Event::Roost { value })
            .chain([Event::Dirty, Event::Flush])
            .collect())
    }
    pub fn replay_story(&mut self, id: &str) -> Result<Vec<Event>, Error> {
        self.require_roost()?;
        if !content::story_happened(self.profile(), id) {
            return Err(Error::LockedMission);
        }
        let page = content::story_page(id)
            .ok_or(Error::UnknownMission)?
            .clone();
        Ok(self.open_story(id, page, "The story so far", Next::Roost))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn adventure() -> Adventure {
        let mut profile = crate::profile::normalize_profile(&json!({"build":{"name":"Nova","dname":"Ember","path":"forge"},"earned":200,"cleared":["m2","m3"]}), "rider3", 2, || "extra".into()).unwrap();
        profile["seenIntro"] = json!(true);
        let (mut a, _) = Adventure::new(
            profile,
            vec![],
            Viewport::from_screen(800., 600., 1., 130., 0.62),
            42,
            2,
        )
        .unwrap();
        a.gate.state = "roost".into();
        a
    }
    #[test]
    fn leaving_mission_does_not_bank_carried_gems() {
        let mut a = adventure();
        a.profile["briefed"] = json!(["m1"]);
        a.choose_mission("m1", false).unwrap();
        let earned = a.profile()["earned"].clone();
        a.game_mut().unwrap().mission.carry = 37.;
        a.pause();
        a.leave_mission().unwrap();
        assert_eq!(a.state(), "map");
        assert!(a.game().is_none());
        assert_eq!(a.profile()["earned"], earned);
        assert!(a.leave_mission().is_err());
    }
    #[test]
    fn charms_spend_exact_cost_and_stop_at_three() {
        let mut a = adventure();
        let clock = Clock {
            now_ms: 100,
            day: 0,
        };
        for _ in 0..3 {
            a.buy_charm("heart", clock).unwrap();
        }
        assert_eq!(a.profile()["charms"]["heart"], 3);
        assert_eq!(a.profile()["spent"], 75.);
        let before = a.profile().clone();
        assert!(a.buy_charm("heart", clock).is_err());
        assert!(a.buy_charm("bogus", clock).is_err());
        assert_eq!(a.profile(), &before);
    }
    #[test]
    fn loadout_obeys_source_fallback_capacity_and_last_word() {
        let mut a = adventure();
        assert_eq!(a.active_loadout(), vec!["naina", "thuri"]);
        assert!(a.toggle_word("kaelo").is_err());
        a.toggle_word("thuri").unwrap();
        assert!(a.toggle_word("naina").is_err());
        a.toggle_word("kaelo").unwrap();
        assert_eq!(a.active_loadout(), vec!["naina", "kaelo"]);
        assert!(a.toggle_word("faro").is_err());
        a.profile["cleared"]
            .as_array_mut()
            .unwrap()
            .push(json!("m12"));
        assert_eq!(a.loadout_slots(), 3);
        a.toggle_word("thuri").unwrap();
    }
    #[test]
    fn release_protects_bond_and_journal_protects_locked_story() {
        let mut a = adventure();
        let clock = Clock {
            now_ms: 100,
            day: 0,
        };
        assert!(a.release_dragon(0, clock).is_err());
        a.profile["party"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"extra","name":"Spark","kind":"ember","color":"#ff8800","xp":0}));
        a.profile["main"] = json!("extra");
        a.release_dragon(1, clock).unwrap();
        assert_eq!(a.profile()["main"], "bond");
        assert_eq!(a.profile()["wild"][0]["name"], "Spark");
        assert_eq!(a.profile()["released"], 1.);
        assert!(a.replay_story("m24").is_err());
        a.replay_story("intro").unwrap();
        assert_eq!(a.state(), "story");
        assert!(a.buy_charm("heart", clock).is_err());
    }
    #[test]
    #[ignore = "requires macOS JavaScriptCore source oracle"]
    fn original_charm_and_loadout_actions_match() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let source = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/dragon/index.html"
        ))
        .unwrap();
        let block = |start: &str, end: &str| {
            let a = source.find(start).unwrap();
            let b = a + source[a..].find(end).unwrap() + end.len();
            &source[a..b]
        };
        let mut script = String::from(
            "let me;const markDirty=()=>{},renderSpells=()=>{},toast=()=>{},plog=()=>{};const sfx=new Proxy({}, {get:()=>()=>{}});const hoard=()=>Math.max(0,Math.floor(me.earned-me.spent));Date.now=()=>100;\n",
        );
        for (start, end) in [
            ("const CHARMS = [", "\n];"),
            ("const RANKS = [", "\n];"),
            ("const WORDS = {", "\n};"),
            ("const rankOf =", "\n"),
            ("const knownWords =", "\n"),
            ("const spellSlots =", "\n"),
            ("function activeWords(", "\n}"),
            ("function toggleWord(", "\n}"),
            ("function spend(", "\n}"),
            ("function buyCharm(", "\n"),
        ] {
            script.push_str(block(start, end));
            script.push('\n');
        }
        let commands = json!([
            ["charm", "heart"],
            ["charm", "heart"],
            ["charm", "heart"],
            ["charm", "heart"],
            ["word", "kaelo"],
            ["word", "thuri"],
            ["word", "naina"],
            ["word", "kaelo"],
            ["word", "faro"],
            ["charm", "pouch"],
            ["charm", "start"]
        ]);
        let mut cases = Vec::new();
        let mut expected = Vec::new();
        for gems in [0., 24., 100., 1000.] {
            for rank3 in [false, true] {
                let mut a = adventure();
                a.profile["earned"] = json!(gems);
                if rank3 {
                    a.profile["cleared"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("m12"));
                }
                cases.push(a.profile().clone());
                let mut snapshots = Vec::new();
                for action in commands.as_array().unwrap() {
                    if action[0] == "charm" {
                        let _ = a.buy_charm(
                            action[1].as_str().unwrap(),
                            Clock {
                                now_ms: 100,
                                day: 0,
                            },
                        );
                    } else {
                        let _ = a.toggle_word(action[1].as_str().unwrap());
                    }
                    snapshots.push(a.profile().clone());
                }
                expected.push(snapshots);
            }
        }
        script.push_str(&format!("const cases={},commands={};print(JSON.stringify(cases.map(p=>{{me=p;return commands.map(c=>{{if(c[0]==='charm')buyCharm(c[1]);else toggleWord(c[1]);return JSON.parse(JSON.stringify(me));}});}})));",json!(cases),commands));
        let mut child = Command::new(
            "/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc",
        )
        .args(["-e", "eval(JSON.parse(readline()))"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
        writeln!(
            child.stdin.take().unwrap(),
            "{}",
            serde_json::to_string(&script).unwrap()
        )
        .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
        fn numeric(v: &mut Value) {
            match v {
                Value::Number(n) => *n = serde_json::Number::from_f64(n.as_f64().unwrap()).unwrap(),
                Value::Array(a) => a.iter_mut().for_each(numeric),
                Value::Object(o) => o.values_mut().for_each(numeric),
                _ => {}
            }
        }
        let mut actual = actual;
        let mut expected = json!(expected);
        numeric(&mut actual);
        numeric(&mut expected);
        for (case, (actual, expected)) in actual
            .as_array()
            .unwrap()
            .iter()
            .zip(expected.as_array().unwrap())
            .enumerate()
        {
            for (step, (actual, expected)) in actual
                .as_array()
                .unwrap()
                .iter()
                .zip(expected.as_array().unwrap())
                .enumerate()
            {
                assert_eq!(actual, expected, "case {case}, step {step}");
            }
        }
    }
}
