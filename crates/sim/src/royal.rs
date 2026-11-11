//! Personal castle quests and Data-Driven practice. No table data enters the sim.
//! The ledger supplies completed-work receipts; ordinary movement runs the bell trial.
use crate::level::royal as map;
use crate::{PlayerId, Sim, SimCrab, Skill, TICKS_PER_SECOND, Tuning};
use bevy_math::Vec2;
use serde::{Deserialize, Serialize};

pub const DATA_LEVEL_STEP: f32 = 20.0;
pub const TRIAL_LIMIT: u32 = 180 * TICKS_PER_SECOND;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Progress {
    pub rooms: u16,
    pub index_started: bool,
    pub pages: u8,
    pub index_returned: bool,
    pub puzzles: u8,
    pub best_bell_ticks: Option<u32>,
    pub tokens: u32,
    pub ribbons: u8,
    pub wearing: Option<u8>,
    pub uploaded: bool,
    pub tournaments: u32,
    pub judged: u64,
    pub research_serial: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BellRun {
    pub elapsed: u32,
    pub next: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    TalkArchivist,
    StartBells,
    Answer { puzzle: u8, choice: u8 },
    Ribbon(u8),
    UnequipRibbon,
    Upload { rows: u16, columns: u8 },
    BeginResearch(u64),
    CompleteResearch { ticket: u64, judged: u32 },
    CancelResearch(u64),
}

pub struct Puzzle {
    pub title: &'static str,
    pub question: &'static str,
    pub choices: [&'static str; 3],
    pub answer: u8,
    pub explanation: &'static str,
}
pub const PUZZLES: [Puzzle; 3] = [
    Puzzle {
        title: "The uncertain banquet",
        question: "Choose the largest expected clam delivery. Multiply clams by the chance they arrive.",
        choices: [
            "4 clams · 75% chance",
            "6 clams · 40% chance",
            "2 clams · 100% chance",
        ],
        answer: 0,
        explanation: "4 × 0.75 = 3 expected clams. The other offers average 2.4 and 2. A bigger possible prize is not always a better expectation.",
    },
    Puzzle {
        title: "The constrained crown",
        question: "The crown needs at least 2 pearls and must cost at most 12 shells. Which offer is feasible?",
        choices: [
            "3 pearls · 14 shells",
            "2 pearls · 11 shells",
            "1 pearl · 4 shells",
        ],
        answer: 1,
        explanation: "Only 2 pearls for 11 shells satisfies both constraints. A bargain that misses the goal is still infeasible.",
    },
    Puzzle {
        title: "The suspicious sample",
        question: "A clerk predicts every crab is red after seeing only the king's three red guards. What should the court do next?",
        choices: [
            "Crown the model immediately",
            "Paint every crab red",
            "Test it on crabs outside the guard",
        ],
        answer: 2,
        explanation: "Use a separate, representative test sample. A perfect fit to three selected guards does not establish performance on the rest of the kingdom.",
    },
];

pub fn nearby(pos: Vec2, at: Vec2) -> bool {
    (pos.x - at.x).abs() < 135.0 && (pos.y - at.y).abs() < 38.0
}
pub fn rank(level: u32) -> &'static str {
    match level {
        1 => "Curious Crab",
        2..=4 => "Junior Analyst",
        5..=9 => "Royal Scholar",
        10..=24 => "Court Auditor",
        _ => "Oracle of the Lists",
    }
}
fn earn(crab: &mut SimCrab, xp: f32, tuning: &Tuning) {
    crab.pending.levelled = crab
        .mover
        .earn(Skill::DataDriven, xp, tuning)
        .or(crab.pending.levelled);
}
pub(crate) fn apply(crab: &mut SimCrab, action: Action, tuning: &Tuning) -> bool {
    if crab.remote
        || crab.fighter.dead.is_some()
            && !matches!(
                action,
                Action::CompleteResearch { .. } | Action::CancelResearch(_)
            )
    {
        return false;
    }
    let beside = |at| crab.mover.grounded && nearby(crab.mover.pos, at);
    match action {
        Action::TalkArchivist if beside(map::ARCHIVIST) => {
            if crab.royal.index_started && crab.royal.pages == 7 && !crab.royal.index_returned {
                crab.royal.index_returned = true;
                crab.royal.tokens = crab.royal.tokens.saturating_add(5);
            } else {
                crab.royal.index_started = true;
            }
        }
        Action::StartBells if beside(map::BELL_KEEPER) => {
            crab.bell_run = Some(BellRun {
                elapsed: 0,
                next: 0,
            });
        }
        Action::Answer { puzzle, choice } if beside(map::OBSERVATORY) => {
            let Some(p) = PUZZLES.get(usize::from(puzzle)) else {
                return false;
            };
            if choice != p.answer || crab.royal.puzzles & (1 << puzzle) != 0 {
                return false;
            }
            crab.royal.puzzles |= 1 << puzzle;
            crab.royal.tokens = crab.royal.tokens.saturating_add(2);
            earn(crab, 20.0, tuning);
        }
        Action::Ribbon(color) if beside(map::WARDROBE) && color < 4 => {
            let bit = 1 << color;
            if crab.royal.ribbons & bit == 0 {
                if crab.royal.tokens < 3 {
                    return false;
                }
                crab.royal.tokens -= 3;
                crab.royal.ribbons |= bit;
            }
            crab.royal.wearing = Some(color);
        }
        Action::UnequipRibbon if beside(map::WARDROBE) => {
            crab.royal.wearing = None;
        }
        Action::Upload { rows, columns } if beside(Vec2::new(map::BOOK.x, map::FLOOR_Y)) => {
            if !(3..=5000).contains(&rows) || !(2..=64).contains(&columns) || crab.royal.uploaded {
                return false;
            }
            crab.royal.uploaded = true;
            earn(crab, 40.0, tuning);
        }
        Action::BeginResearch(ticket) if beside(Vec2::new(map::BOOK.x, map::FLOOR_Y)) => {
            let Some(next) = crab.royal.research_serial.checked_add(1) else {
                return false;
            };
            if ticket == 0 || ticket != next || crab.research_ticket.is_some() {
                return false;
            }
            crab.royal.research_serial = ticket;
            crab.research_ticket = Some(ticket);
        }
        Action::CompleteResearch { ticket, judged } => {
            if crab.research_ticket != Some(ticket) || judged == 0 {
                return false;
            }
            crab.research_ticket = None;
            crab.royal.tournaments = crab.royal.tournaments.saturating_add(1);
            crab.royal.judged = crab.royal.judged.saturating_add(u64::from(judged));
            // Real repeated practice counts, like climbing. Starting/cancelling does not.
            earn(crab, 10.0 + judged.min(256) as f32 * 1.5, tuning);
        }
        Action::CancelResearch(ticket) => {
            if crab.research_ticket != Some(ticket) {
                return false;
            }
            crab.research_ticket = None;
        }
        _ => return false,
    }
    true
}

pub(crate) fn step(crab: &mut SimCrab) {
    if crab.remote {
        return;
    }
    if crab.fighter.dead.is_some() || !map::in_castle(crab.mover.pos) {
        crab.bell_run = None;
        return;
    }
    if let Some(room) = map::room_at(crab.mover.pos) {
        let bit = 1 << room;
        if crab.royal.rooms & bit == 0 {
            crab.royal.rooms |= bit;
            crab.royal.tokens = crab.royal.tokens.saturating_add(1);
        }
    }
    if crab.royal.index_started {
        for (i, page) in map::INDEX_PAGES.iter().enumerate() {
            if crab.mover.pos.distance(*page) < 100.0 {
                crab.royal.pages |= 1 << i;
            }
        }
    }
    if let Some(run) = &mut crab.bell_run {
        run.elapsed = run.elapsed.saturating_add(1);
        if run.elapsed > TRIAL_LIMIT {
            crab.bell_run = None;
            return;
        }
        if let Some(at) = map::BELLS.get(usize::from(run.next))
            && crab.mover.pos.distance(*at) < 110.0
        {
            run.next += 1;
        }
        if usize::from(run.next) == map::BELLS.len() {
            if crab.royal.best_bell_ticks.is_none() {
                crab.royal.tokens = crab.royal.tokens.saturating_add(4);
            }
            crab.royal.best_bell_ticks = Some(
                crab.royal
                    .best_bell_ticks
                    .map_or(run.elapsed, |b| b.min(run.elapsed)),
            );
            crab.bell_run = None;
        }
    }
}

impl Sim {
    /// A local, replayable castle command. It never transmits workbook contents.
    pub fn royal_action(&mut self, id: PlayerId, action: Action) -> bool {
        let tuning = self.tuning.clone();
        let Some(crab) = self.crab_mut(id) else {
            return false;
        };
        if !apply(crab, action, &tuning) {
            return false;
        }
        self.note_royal(action);
        true
    }
}
