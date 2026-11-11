//! Offline/static adventure flow for an already-built normalized profile.
//! Lobby synchronization, builder, narration playback/heard-page history and storage
//! are host responsibilities. Other profiles provide the source's shared unlocks;
//! they do not turn the current solo Game into a multiplayer simulation.
use crate::{
    content,
    flight::{Input, Viewport},
    game::Game,
    progression,
    random::Mulberry32,
    roost::{self, Roost},
    roost_gate::{self, Gate},
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pub now_ms: u64,
    pub day: i64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    BuilderRequired,
    InvalidProfile,
    LockedMission,
    UnknownMission,
    InvalidState,
    Runtime(&'static str),
    Progression(progression::Error),
    Roost(roost::Error),
    Gate(roost_gate::Error),
}
impl From<roost::Error> for Error {
    fn from(e: roost::Error) -> Self {
        Self::Roost(e)
    }
}
impl From<roost_gate::Error> for Error {
    fn from(e: roost_gate::Error) -> Self {
        Self::Gate(e)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Dirty,
    Flush,
    Blur,
    StopNarration,
    ClearControls,
    ClearField,
    UpdateHud,
    Screen {
        name: Option<String>,
    },
    Story {
        id: String,
        page: Value,
        kicker: String,
    },
    Sound {
        name: String,
    },
    PauseOverlay {
        visible: bool,
    },
    Results {
        won: bool,
    },
    Progression {
        value: progression::Event,
    },
    Roost {
        value: roost::Event,
    },
    Gate {
        value: roost_gate::Event,
    },
    Completion {
        value: roost::CompletionEvent,
    },
}
#[derive(Clone, Debug)]
enum Next {
    Party,
    Map,
    Roost,
    Launch(String),
    Results,
    Rank(usize),
    Skills,
}
#[derive(Clone, Debug)]
struct Story {
    id: String,
    next: Next,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResultSummary {
    pub won: bool,
    pub mission: String,
    pub banked: f64,
    pub lost: f64,
    pub kills: usize,
    pub bonks: usize,
}
pub struct Adventure {
    initial: Value,
    profile: Value,
    pub others: Vec<Value>,
    pub game: Option<Game>,
    pub gate: Gate,
    pub roost: Roost,
    pub result: Option<ResultSummary>,
    pub reduce_motion: bool,
    rng: Mulberry32,
    view: Viewport,
    story: Option<Story>,
    milestone_visible: bool,
}
fn has(p: &Value, key: &str, id: &str) -> bool {
    p[key].as_array().is_some_and(|a| a.iter().any(|v| v == id))
}
impl Adventure {
    /// Starts at source pickRider's post-load built-profile boundary. The caller
    /// must handle menu freeze/loading/builder/lobby outside this offline session.
    pub fn new(
        profile: Value,
        others: Vec<Value>,
        view: Viewport,
        seed: u32,
        rider_index: usize,
    ) -> Result<(Self, Vec<Event>), Error> {
        if profile["build"].is_null() {
            return Err(Error::BuilderRequired);
        }
        if !profile["build"].is_object()
            || !profile["party"].is_array()
            || profile["party"].as_array().is_some_and(Vec::is_empty)
            || !["briefed", "seenGrow", "cleared", "seenMs"]
                .iter()
                .all(|k| profile[k].is_array())
        {
            return Err(Error::InvalidProfile);
        }
        let mut a = Self {
            initial: profile.clone(),
            profile,
            others,
            game: None,
            gate: Gate::new(rider_index, false),
            roost: Roost::default(),
            result: None,
            reduce_motion: false,
            rng: Mulberry32 { state: seed },
            view,
            story: None,
            milestone_visible: false,
        };
        let mut events = a.to_party()?;
        if a.profile["seenIntro"] != true {
            events.extend(a.open_story(
                "intro",
                content::story_page("intro").expect("source intro").clone(),
                "Riders of Valmora",
                Next::Party,
            ));
        }
        Ok((a, events))
    }
    /// Active flights own their profile directly; no whole-profile clone per frame.
    pub fn profile(&self) -> &Value {
        self.game
            .as_ref()
            .and_then(|g| g.profile.as_ref())
            .unwrap_or(&self.profile)
    }
    fn retire_game(&mut self) {
        if let Some(g) = self.game.take() {
            if let Some(p) = g.profile {
                self.profile = p;
            }
            self.rng = g.rng;
        }
    }
    pub fn initial_profile(&self) -> &Value {
        &self.initial
    }
    /// Apply finishBuild's edit branch without resetting adventure RNG or original import.
    pub fn change_look(&mut self, draft: &Value, now_ms: u64) -> Result<Vec<Event>, Error> {
        if !matches!(self.state(), "roost" | "skills") || self.game.is_some() {
            return Err(Error::InvalidState);
        }
        let build = crate::profile::clean_build(draft).map_err(|_| Error::InvalidProfile)?;
        if !build.is_object() {
            return Err(Error::InvalidProfile);
        }
        let dragon = self.profile["party"]
            .as_array_mut()
            .and_then(|party| party.first_mut())
            .and_then(Value::as_object_mut)
            .ok_or(Error::InvalidProfile)?;
        for (to, from) in [
            ("name", "dname"),
            ("color", "dcolor"),
            ("horn", "horn"),
            ("kind", "dkind"),
        ] {
            dragon.insert(to.into(), build[from].clone());
        }
        self.profile["build"] = build;
        self.profile["last"] = json!({"text":"changed their look","at":now_ms});
        let mut events = vec![
            Event::Dirty,
            Event::Flush,
            Event::Sound {
                name: "pick".into(),
            },
        ];
        events.extend(self.to_roost()?);
        Ok(events)
    }
    pub fn state(&self) -> &str {
        &self.gate.state
    }
    pub fn story_id(&self) -> Option<&str> {
        self.story.as_ref().map(|s| s.id.as_str())
    }
    pub fn mission_unlocked(&self, id: &str) -> bool {
        content::mission_unlocked(self.profile(), &self.others, id)
    }
    fn screen(&mut self, state: &str, screen: Option<&str>) -> Event {
        self.gate.state = state.into();
        Event::Screen {
            name: screen.map(str::to_owned),
        }
    }
    fn open_story(&mut self, id: &str, page: Value, kicker: &str, next: Next) -> Vec<Event> {
        self.story = Some(Story {
            id: id.into(),
            next,
        });
        self.gate.state = "story".into();
        vec![
            Event::Story {
                id: id.into(),
                page,
                kicker: kicker.into(),
            },
            Event::Screen {
                name: Some("storyScreen".into()),
            },
            Event::Sound {
                name: "spell".into(),
            },
        ]
    }
    fn part2(&mut self, next: Next) -> Option<Vec<Event>> {
        if !has(&self.profile, "cleared", "m12") || self.profile["seenPart2"] == true {
            return None;
        }
        self.profile["seenPart2"] = json!(true);
        let mut e = vec![Event::Dirty, Event::Flush];
        e.extend(self.open_story(
            "part2",
            content::story_page("part2").expect("part2").clone(),
            "A new adventure begins",
            next,
        ));
        Some(e)
    }
    pub fn to_party(&mut self) -> Result<Vec<Event>, Error> {
        self.retire_game();
        if let Some(e) = self.part2(Next::Party) {
            return Ok(e);
        }
        self.game = None;
        let screen = self.screen("party", Some("partyScreen"));
        Ok(vec![
            Event::ClearField,
            screen,
            Event::UpdateHud,
            Event::Flush,
        ])
    }
    pub fn to_map(&mut self) -> Result<Vec<Event>, Error> {
        self.retire_game();
        if let Some(e) = self.part2(Next::Map) {
            return Ok(e);
        }
        self.game = None;
        let screen = self.screen("map", Some("mapScreen"));
        Ok(vec![
            Event::ClearField,
            screen,
            Event::UpdateHud,
            Event::Flush,
        ])
    }
    fn grow_scene(&mut self) -> Option<Vec<Event>> {
        let xp = self.profile["party"][0]["xp"].as_f64().unwrap_or(0.0);
        let level = crate::level_of(xp);
        let team = progression::team_summary(&self.profile, &[], true);
        let riding = progression::riding(&self.profile, &team);
        let id = [
            ("glider", level >= 4),
            ("grown", level >= 8),
            ("ride", riding),
            ("mighty", level >= 12),
        ]
        .into_iter()
        .find(|(id, ready)| *ready && !has(&self.profile, "seenGrow", id))?
        .0;
        let page = content::catalog()["grow_scenes"]
            .as_array()
            .expect("source growth scenes")
            .iter()
            .find(|g| g["id"] == id)
            .expect("growth page")
            .clone();
        self.profile["seenGrow"]
            .as_array_mut()
            .expect("normalized")
            .push(json!(id));
        let mut e = vec![Event::Dirty];
        e.extend(self.open_story("grow", page, "Growing up", Next::Roost));
        Some(e)
    }
    pub fn to_roost(&mut self) -> Result<Vec<Event>, Error> {
        self.retire_game();
        if let Some(e) = self.grow_scene() {
            return Ok(e);
        }
        self.game = None;
        let screen = self.screen("roost", Some("roostScreen"));
        let mut e = vec![
            Event::StopNarration,
            Event::ClearField,
            screen,
            Event::UpdateHud,
            Event::Flush,
        ];
        let milestones = self.roost.check_milestones(
            &mut self.profile,
            &[],
            true,
            true,
            !self.milestone_visible,
        )?;
        if milestones
            .iter()
            .any(|e| matches!(e, roost::Event::ShowMilestone { .. }))
        {
            self.milestone_visible = true;
        }
        e.extend(milestones.into_iter().map(|value| Event::Roost { value }));
        Ok(e)
    }
    /// Called when the host's narration/read gate permits Continue. This executes
    /// source closeStory; it does not claim narration completed or skip its lock.
    pub fn close_story(&mut self, can_continue: bool) -> Result<Vec<Event>, Error> {
        if self.state() != "story" || !can_continue {
            return Ok(Vec::new());
        }
        let s = self.story.take().ok_or(Error::InvalidState)?;
        let mut e = vec![Event::StopNarration];
        if s.id == "intro" && self.profile["seenIntro"] != true {
            self.profile["seenIntro"] = json!(true);
            e.push(Event::Dirty);
        }
        e.push(Event::Blur);
        e.extend(match s.next {
            Next::Party => self.to_party()?,
            Next::Map => self.to_map()?,
            Next::Roost => self.to_roost()?,
            Next::Launch(id) => self.launch(&id, false)?,
            Next::Results => self.show_results()?,
            Next::Rank(index) => self.show_rank(index),
            Next::Skills => vec![
                self.screen("skills", Some("skillsScreen")),
                Event::UpdateHud,
            ],
        });
        Ok(e)
    }
    /// Offline joinOrStart: live host joining and party readiness are outside this
    /// module. from_party is only for an already-authorized Party launch.
    pub fn choose_mission(&mut self, id: &str, from_party: bool) -> Result<Vec<Event>, Error> {
        let defs = crate::missions::missions();
        let base = id.strip_suffix('c').unwrap_or(id);
        let def = defs
            .iter()
            .find(|m| m.id == base)
            .ok_or(Error::UnknownMission)?;
        if !from_party && !self.mission_unlocked(id) {
            return Err(Error::LockedMission);
        }
        if !from_party
            && !has(&self.profile, "briefed", id)
            && let Some(text) = content::briefing(id)
        {
            let alone = content::catalog()["no_wick_briefs"]
                .as_array()
                .expect("source no-wick")
                .iter()
                .any(|v| v == id);
            return Ok(self.open_story(
                id,
                json!({"title":def.name,"text":[text],"alone":alone,"clip":format!("brief-{id}"),"next":content::mission_goal(id)}),
                "Your next mission",
                Next::Launch(id.into()),
            ));
        }
        self.launch(id, from_party)
    }
    fn launch(&mut self, id: &str, from_party: bool) -> Result<Vec<Event>, Error> {
        if !from_party && !self.mission_unlocked(id) {
            return Err(Error::LockedMission);
        }
        self.retire_game();
        let mut g = Game::from_profile(id, self.profile.clone(), self.view, self.rng.state)
            .map_err(Error::Runtime)?;
        let p = g.profile.as_mut().expect("profile game");
        if content::briefing(id).is_some() && !has(p, "briefed", id) {
            p["briefed"]
                .as_array_mut()
                .expect("normalized")
                .push(json!(id));
        }
        self.profile = p.clone();
        self.rng = g.rng;
        self.game = Some(g);
        self.result = None;
        self.story = None;
        let screen = self.screen("play", None);
        Ok(vec![
            Event::ClearField,
            Event::Dirty,
            screen,
            Event::UpdateHud,
        ])
    }
    pub fn pause(&mut self) -> Vec<Event> {
        if self.state() != "play" {
            return Vec::new();
        }
        self.gate.state = "pause".into();
        vec![
            Event::ClearControls,
            Event::PauseOverlay { visible: true },
            Event::Flush,
        ]
    }
    pub fn resume(&mut self) -> Vec<Event> {
        if self.state() != "pause" {
            return Vec::new();
        }
        self.gate.state = "play".into();
        vec![Event::PauseOverlay { visible: false }, Event::Blur]
    }
    fn show_results(&mut self) -> Result<Vec<Event>, Error> {
        let won = self.result.as_ref().ok_or(Error::InvalidState)?.won;
        let screen = self.screen("results", Some("resultsScreen"));
        Ok(vec![Event::Results { won }, screen, Event::UpdateHud])
    }
    fn show_rank(&mut self, index: usize) -> Vec<Event> {
        let rank = &content::catalog()["ranks"][index];
        let title = format!("New rank: {}", rank["name"].as_str().unwrap_or(""));
        self.open_story(
            "rank",
            json!({"title":title,"text":[rank["scene"]],"clip":format!("rank-{index}")}),
            "You are growing up",
            Next::Results,
        )
    }
    fn finish_game(&mut self) -> Result<Vec<Event>, Error> {
        let g = self.game.take().ok_or(Error::InvalidState)?;
        let won = g.result.ok_or(Error::InvalidState)?;
        self.profile = g.profile.ok_or(Error::InvalidProfile)?;
        self.rng = g.rng;
        self.result = Some(ResultSummary {
            won,
            mission: g.definition.id.clone(),
            banked: g.mission.banked,
            lost: g.lost,
            kills: g.kills,
            bonks: g.bonks,
        });
        let mut e = vec![Event::ClearControls];
        let mut continuation = None;
        for value in g.completion_events {
            match value {
                roost::CompletionEvent::Results { story, rank } => {
                    continuation = Some((story, rank))
                }
                roost::CompletionEvent::Dirty => e.push(Event::Dirty),
                roost::CompletionEvent::Flush => e.push(Event::Flush),
                value => e.push(Event::Completion { value }),
            }
        }
        let (story, rank) = continuation.ok_or(Error::InvalidState)?;
        if let Some(id) = story {
            if let Some(page) = content::story_page(&id) {
                e.extend(self.open_story(
                    &id,
                    page.clone(),
                    "Mission complete! The story continues",
                    rank.map_or(Next::Results, Next::Rank),
                ));
            } else {
                e.extend(self.show_results()?);
            }
        } else {
            e.extend(self.show_results()?);
        }
        Ok(e)
    }
    pub fn tick(
        &mut self,
        elapsed: f64,
        input: Input,
        view: Viewport,
        clock: Clock,
    ) -> Result<Vec<Event>, Error> {
        self.view = view;
        let active = if let Some(g) = self.game.as_mut() {
            g.profile.as_mut().ok_or(Error::InvalidProfile)?
        } else {
            &mut self.profile
        };
        let gate_events = self
            .gate
            .advance(active, clock.now_ms as f64, || self.rng.next_unit())?;
        let mut e = self.gate_effects(gate_events, clock)?;
        if self.state() == "play" {
            let g = self.game.as_mut().ok_or(Error::InvalidState)?;
            g.completion_day = clock.day;
            g.completion_now_ms = clock.now_ms;
            g.step(elapsed, input, view).map_err(Error::Runtime)?;
            if g.profile.is_none() {
                return Err(Error::InvalidProfile);
            }
            self.rng = g.rng;
            if g.result.is_some() {
                e.extend(self.finish_game()?);
            }
        }
        if self.gate.frozen.is_some() {
            let freeze = self.gate.update_freeze(clock.now_ms as f64)?;
            e.extend(self.gate_effects(freeze, clock)?);
        }
        Ok(e)
    }
    pub fn wake_egg(&mut self, clock: Clock) -> Result<Vec<Event>, Error> {
        let active = self
            .game
            .as_ref()
            .and_then(|g| g.profile.as_ref())
            .unwrap_or(&self.profile);
        let e = self
            .gate
            .begin(active, clock.now_ms as f64, || self.rng.next_unit())?;
        self.gate_effects(e, clock)
    }
    pub fn times_key(&mut self, key: &str) -> Vec<Event> {
        self.gate
            .key(key)
            .into_iter()
            .map(|value| Event::Gate { value })
            .collect()
    }
    pub fn answer_times(&mut self, clock: Clock) -> Result<Vec<Event>, Error> {
        let e = self.gate.answer(clock.now_ms as f64)?;
        self.gate_effects(e, clock)
    }
    fn gate_effects(
        &mut self,
        events: Vec<roost_gate::Event>,
        clock: Clock,
    ) -> Result<Vec<Event>, Error> {
        let mut e = Vec::new();
        for value in events {
            match value {
                roost_gate::Event::Hatch => {
                    self.retire_game();
                    let hatch = self.roost.begin_hatch(
                        &self.profile,
                        &[],
                        true,
                        clock.now_ms,
                        self.reduce_motion,
                        || self.rng.next_unit(),
                    )?;
                    e.extend(hatch.into_iter().map(|value| Event::Roost { value }));
                }
                roost_gate::Event::ToRoost => e.extend(self.to_roost()?),
                value => e.push(Event::Gate { value }),
            }
        }
        Ok(e)
    }
    pub fn finish_hatch(
        &mut self,
        name: &str,
        release_id: Option<&str>,
        clock: Clock,
    ) -> Result<Vec<Event>, Error> {
        if self.state() != "hatch" {
            return Ok(Vec::new());
        }
        let events = self.roost.finish_hatch(
            &mut self.profile,
            name,
            release_id,
            self.gate.rider_index,
            clock.now_ms,
        )?;
        let mut e = Vec::new();
        for value in events {
            if value == roost::Event::ToRoost {
                e.extend(self.to_roost()?);
            } else {
                e.push(Event::Roost { value });
            }
        }
        Ok(e)
    }
    pub fn close_milestone(&mut self) -> Result<Vec<Event>, Error> {
        let values = self
            .roost
            .close_milestone(&mut self.profile, self.gate.state == "roost")?;
        for value in &values {
            match value {
                roost::Event::ShowMilestone { .. } => self.milestone_visible = true,
                roost::Event::HideMilestone => self.milestone_visible = false,
                _ => {}
            }
        }
        Ok(values
            .into_iter()
            .map(|value| Event::Roost { value })
            .collect())
    }
    pub fn update_freeze(&mut self, clock: Clock) -> Result<Vec<Event>, Error> {
        let e = self.gate.update_freeze(clock.now_ms as f64)?;
        self.gate_effects(e, clock)
    }
    pub fn thaw(&mut self, clock: Clock) -> Result<Vec<Event>, Error> {
        let e = self.gate.finish_freeze(clock.now_ms as f64)?;
        self.gate_effects(e, clock)
    }
    /// Roost actions reuse the existing command semantics; story callbacks remain
    /// in this layer rather than being performed by progression itself.
    pub fn roost_command(
        &mut self,
        command: progression::Command,
        clock: Clock,
    ) -> Result<Vec<Event>, Error> {
        if self.state() != "roost" && self.state() != "skills" {
            return Err(Error::InvalidState);
        }
        let grow = matches!(
            command,
            progression::Command::Feed(_) | progression::Command::Treat(_)
        );
        let learn = matches!(command, progression::Command::LearnSkill(_));
        let values = progression::apply(&mut self.profile, &[], true, command, clock.now_ms as f64)
            .map_err(Error::Progression)?;
        let mut e = Vec::new();
        let mut lesson = None;
        for value in values {
            if let progression::Event::Learned { name, .. } = &value {
                lesson = Some(name.clone());
            }
            e.push(Event::Progression { value });
        }
        e.extend([Event::Dirty, Event::Flush]);
        if !learn {
            e.push(Event::UpdateHud);
        }
        if learn && let Some(name) = lesson {
            let text = content::catalog()["skill_stories"][&name].clone();
            e.extend(self.open_story(
                "lesson",
                json!({"title":name,"text":[text]}),
                &format!(
                    "{} learned something new",
                    self.profile["build"]["name"].as_str().unwrap_or("")
                ),
                Next::Skills,
            ));
        } else if grow && let Some(story) = self.grow_scene() {
            e.extend(story);
        }
        Ok(e)
    }
}
