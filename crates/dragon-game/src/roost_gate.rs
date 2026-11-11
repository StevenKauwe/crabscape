//! Pure times-table and freeze controller. The host supplies time and RNG and
//! interprets UI/save intents. Nothing reads storage, speaks, or schedules OS timers.
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Question {
    pub a: u8,
    pub b: u8,
    pub n: u16,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    ClearControls,
    Sound { name: String },
    CancelSpeech,
    ShowScreen { name: String },
    HideScreen { name: String },
    Dirty,
    SaveFrozen { value: Value },
    Hatch,
    Blur,
    ToRoost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidTime,
    InvalidRandom,
    ExhaustedQuestions,
    InvalidFrozen,
    UnsupportedCoercion,
}
#[derive(Clone, Debug)]
enum Callback {
    Unlock(u64),
    Ask,
    Hatch,
    Wrong(Question),
}
#[derive(Clone, Debug)]
struct Timer {
    at: f64,
    callback: Callback,
}
#[derive(Clone, Debug)]
pub struct Gate {
    pub state: String,
    pub question: Option<Question>,
    pub done: bool,
    pub run: u32,
    pub seen: Vec<u16>,
    pub input: String,
    pub locked: bool,
    pub card_locked: bool,
    pub verdict: String,
    pub verdict_class: String,
    pub frozen: Option<Value>,
    pub thaw_hidden: bool,
    pub freeze_hidden: bool,
    pub freeze_title: String,
    pub freeze_note: String,
    pub freeze_clock: String,
    pub test_flight: bool,
    pub rider_index: usize,
    generation: u64,
    timers: Vec<Timer>,
}
impl Default for Gate {
    fn default() -> Self {
        Self {
            state: "menu".into(),
            question: None,
            done: false,
            run: 0,
            seen: Vec::new(),
            input: String::new(),
            locked: false,
            card_locked: false,
            verdict: String::new(),
            verdict_class: "verdict".into(),
            frozen: None,
            thaw_hidden: true,
            freeze_hidden: true,
            freeze_title: String::new(),
            freeze_note: String::new(),
            freeze_clock: String::new(),
            test_flight: false,
            rider_index: 0,
            generation: 0,
            timers: Vec::new(),
        }
    }
}
fn valid_time(now: f64) -> Result<(), Error> {
    if now.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidTime)
    }
}
fn draw(rng: &mut impl FnMut() -> f64) -> Result<u8, Error> {
    let x = rng();
    if x.is_finite() && (0.0..1.0).contains(&x) {
        Ok((x * 13.0).floor() as u8)
    } else {
        Err(Error::InvalidRandom)
    }
}
/// Source dayKey uses the local offset at the supplied instant, including DST.
pub fn day_key(now_ms: f64, timezone_offset_minutes: f64) -> Result<f64, Error> {
    valid_time(now_ms)?;
    valid_time(timezone_offset_minutes)?;
    let d = ((now_ms - timezone_offset_minutes * 60000.0) / 86400000.0).floor();
    if d.is_finite() {
        Ok(d)
    } else {
        Err(Error::InvalidTime)
    }
}
/// Read-only equivalent of loadFrozen. Preserve the supplied document unchanged.
pub fn load_frozen(saved: &Value, now_ms: f64, test_flight: bool) -> Result<Option<Value>, Error> {
    valid_time(now_ms)?;
    if test_flight || !crate::profile::truthy(saved) {
        return Ok(None);
    }
    let until =
        crate::profile::number(saved.get("until")).map_err(|_| Error::UnsupportedCoercion)?;
    Ok(if until > now_ms {
        Some(saved.clone())
    } else {
        None
    })
}
impl Gate {
    pub fn new(rider_index: usize, test_flight: bool) -> Self {
        Self {
            rider_index,
            test_flight,
            ..Self::default()
        }
    }

    pub fn next_question(&mut self, mut rng: impl FnMut() -> f64) -> Result<Question, Error> {
        if self.seen.len() >= 91 {
            return Err(Error::ExhaustedQuestions);
        }
        loop {
            let a = draw(&mut rng)?;
            let b = draw(&mut rng)?;
            let key = u16::from(a.min(b)) * 13 + u16::from(a.max(b));
            if !self.seen.contains(&key) {
                self.seen.push(key);
                return Ok(Question {
                    a,
                    b,
                    n: u16::from(a) * u16::from(b),
                });
            }
        }
    }
    pub fn ask(&mut self, rng: impl FnMut() -> f64) -> Result<(), Error> {
        if self.state != "riddle" {
            return Ok(());
        }
        self.question = Some(self.next_question(rng)?);
        self.generation = self.generation.wrapping_add(1);
        self.done = false;
        self.input.clear();
        self.verdict.clear();
        self.verdict_class = "verdict".into();
        self.locked = false;
        Ok(())
    }
    /// Source eggReady does not cancel older callbacks; unlock checks question identity.
    pub fn begin(
        &mut self,
        profile: &Value,
        now_ms: f64,
        rng: impl FnMut() -> f64,
    ) -> Result<Vec<Event>, Error> {
        valid_time(now_ms)?;
        if !crate::roost::egg_ready(profile) {
            return Ok(Vec::new());
        }
        self.run = 0;
        self.seen.clear();
        self.state = "riddle".into();
        self.ask(rng)?;
        self.card_locked = true;
        self.locked = true;
        self.timers.push(Timer {
            at: now_ms + 1000.0,
            callback: Callback::Unlock(self.generation),
        });
        Ok(vec![
            Event::ClearControls,
            Event::Sound {
                name: "wiggle".into(),
            },
            Event::ShowScreen {
                name: "riddleScreen".into(),
            },
        ])
    }
    /// Source checks the existing UTF-16 length before append, not the new length.
    pub fn key(&mut self, key: &str) -> Vec<Event> {
        if self.state != "riddle" || self.done || self.locked {
            return Vec::new();
        }
        if key == "clear" {
            self.input.clear();
        } else if self.input.encode_utf16().count() < 3 {
            self.input.push_str(key);
        }
        vec![Event::Sound {
            name: "pick".into(),
        }]
    }
    pub fn answer(&mut self, now_ms: f64) -> Result<Vec<Event>, Error> {
        valid_time(now_ms)?;
        if self.done || self.state != "riddle" || self.question.is_none() || self.locked {
            return Ok(Vec::new());
        }
        if self.input.is_empty() {
            self.verdict = "Tap the numbers first.".into();
            self.verdict_class = "verdict".into();
            return Ok(Vec::new());
        }
        let r = self.question.expect("checked");
        let n = crate::profile::number(Some(&json!(self.input)))
            .map_err(|_| Error::UnsupportedCoercion)?;
        self.done = true;
        self.locked = true;
        let mut e = vec![Event::CancelSpeech];
        if n == f64::from(r.n) {
            self.run += 1;
            let done = self.run >= 5;
            self.verdict = if done { "Five in a row!" } else { "Correct!" }.into();
            self.verdict_class = "verdict good".into();
            e.push(Event::Sound {
                name: "correct".into(),
            });
            self.timers.push(Timer {
                at: now_ms + if done { 900.0 } else { 600.0 },
                callback: if done { Callback::Hatch } else { Callback::Ask },
            });
        } else {
            self.verdict = format!("Not quite... {} × {} = {}", r.a, r.b, r.n);
            self.verdict_class = "verdict bad".into();
            e.push(Event::Sound {
                name: "wrong".into(),
            });
            self.timers.push(Timer {
                at: now_ms + 1300.0,
                callback: Callback::Wrong(r),
            });
        }
        Ok(e)
    }
    /// Deliver due callbacks at the actual supplied execution time. In particular,
    /// late wrong-answer callbacks freeze for 60s from execution, not original due time.
    pub fn advance(
        &mut self,
        profile: &mut Value,
        now_ms: f64,
        mut rng: impl FnMut() -> f64,
    ) -> Result<Vec<Event>, Error> {
        valid_time(now_ms)?;
        let mut events = Vec::new();
        loop {
            let next = self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, t)| t.at <= now_ms)
                .min_by(|(ia, a), (ib, b)| a.at.total_cmp(&b.at).then(ia.cmp(ib)))
                .map(|(i, _)| i);
            let Some(i) = next else {
                break;
            };
            let timer = self.timers.remove(i);
            match timer.callback {
                Callback::Unlock(g) => {
                    if g == self.generation {
                        self.card_locked = false;
                        self.locked = self.done;
                    }
                }
                Callback::Ask => self.ask(&mut rng)?,
                Callback::Hatch => {
                    self.state = "hatch".into();
                    events.push(Event::Hatch);
                }
                Callback::Wrong(r) => {
                    profile["eggAwake"] = json!(false);
                    events.extend([
                        Event::HideScreen {
                            name: "riddleScreen".into(),
                        },
                        Event::Dirty,
                    ]);
                    events.extend(self.freeze(json!({"until":now_ms+60000.0,"answer":format!("{} times {} is {}",r.a,r.b,r.n),"who":self.rider_index}),now_ms)?);
                }
            }
        }
        Ok(events)
    }
    pub fn freeze(&mut self, value: Value, now_ms: f64) -> Result<Vec<Event>, Error> {
        valid_time(now_ms)?;
        let until =
            crate::profile::number(value.get("until")).map_err(|_| Error::UnsupportedCoercion)?;
        if !value.is_object() || !until.is_finite() {
            return Err(Error::InvalidFrozen);
        }
        let answer = &value["answer"];
        let note = if crate::profile::truthy(answer) {
            format!(
                "Remember: {}.",
                crate::profile::clean_text(answer).map_err(|_| Error::UnsupportedCoercion)?
            )
        } else {
            String::new()
        };
        let mut events = Vec::new();
        self.frozen = Some(value.clone());
        if !self.test_flight {
            events.push(Event::SaveFrozen { value });
        }
        if self.state != "menu" {
            self.state = "frozen".into();
        }
        events.extend([
            Event::ClearControls,
            Event::Sound {
                name: "freeze".into(),
            },
        ]);
        self.freeze_title = "Frozen solid!".into();
        self.freeze_note = note;
        self.thaw_hidden = true;
        events.extend(self.update_freeze(now_ms)?);
        self.freeze_hidden = false;
        events.push(Event::ShowScreen {
            name: "freezeScreen".into(),
        });
        Ok(events)
    }
    pub fn update_freeze(&mut self, now_ms: f64) -> Result<Vec<Event>, Error> {
        valid_time(now_ms)?;
        let Some(f) = &self.frozen else {
            return Ok(Vec::new());
        };
        let until =
            crate::profile::number(f.get("until")).map_err(|_| Error::UnsupportedCoercion)?;
        let left = ((until - now_ms) / 1000.0).ceil().max(0.0);
        self.freeze_clock = format!(
            "{}:{:02}",
            (left / 60.0).floor() as u64,
            (left % 60.0) as u64
        );
        let mut events = Vec::new();
        if left == 0.0 && self.thaw_hidden {
            events.push(Event::Sound {
                name: "thaw".into(),
            });
            if !self.test_flight {
                events.push(Event::SaveFrozen { value: Value::Null });
            }
            self.freeze_title = "The ice melted!".into();
            if self.state == "frozen" {
                self.thaw_hidden = false;
            } else {
                events.extend(self.finish_freeze(now_ms)?);
            }
        }
        Ok(events)
    }
    pub fn finish_freeze(&mut self, now_ms: f64) -> Result<Vec<Event>, Error> {
        valid_time(now_ms)?;
        let Some(f) = &self.frozen else {
            return Ok(Vec::new());
        };
        let until =
            crate::profile::number(f.get("until")).map_err(|_| Error::UnsupportedCoercion)?;
        if now_ms < until {
            return Ok(Vec::new());
        }
        self.frozen = None;
        self.freeze_hidden = true;
        let mut events = vec![
            Event::HideScreen {
                name: "freezeScreen".into(),
            },
            Event::Blur,
        ];
        if self.state == "frozen" {
            self.state = "roost".into();
            events.push(Event::ToRoost);
        }
        Ok(events)
    }
}
