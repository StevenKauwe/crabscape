//! The first hours: how a brand-new crab's 13 skills level over a sensible,
//! mixed three hours of play, measured by a headless bot on real sim ticks
//! (`common/bot.rs` says how the bot plays and what it assumes), against the
//! owner's targets for levels 1 to 15.
//!
//! The bot is slow (about 2.5 minutes in release for the three hours), so its
//! numbers are cached below, the way `loop_pacing` caches `ROUTES`. Re-measure
//! after any change to nodes, XP, recipes, fights, climbing or swimming:
//! `cargo test --release -p crabsim --test first_hours -- --ignored --nocapture measure_first_hours`
//! and paste the printed tables over `MIXED`, `LEVELUPS_PER_5MIN`,
//! `DISTINCT_PER_5MIN`, `LONG_STRETCHES`, `PRYING_FOCUS`, `GOALS_DONE` and
//! `GOAL_LONGEST_OPEN`.
//!
//! `first_hours_targets` asserts the owner's targets against the cached
//! numbers (met since the 1-15 ladder, XP and early recipes).

#[path = "common/bot.rs"]
mod bot;

use bot::{Bot, Config, SKILLS, Stretch, silent_levels, skill_index};
use crabsim::Skill;

/// Three hours of mixed play.
const MIXED_SECS: f64 = 3.0 * 3600.0;
/// The focused run gives up after this long.
const FOCUS_SECS: f64 = 90.0 * 60.0;

// ---- The cached measurement (paste the printed tables here) ----

/// Minutes to level 5, 10 and 15 for each skill (order `bot::SKILLS`: Climbing,
/// Swimming, Fighting, Foraging, Prying, Excavating, Harvesting, Culturing,
/// Cooking, Shellcraft, Weaving, Building, Survival) in the mixed three hours;
/// -1 is "not by 3 h".
const MIXED: [[f64; 3]; 13] = [
    [11.3, 12.3, 111.1],
    [2.9, 83.7, 136.8],
    [14.1, 88.1, 173.7],
    [37.7, 89.5, 149.6],
    [37.2, 91.5, 160.9],
    [38.2, 77.6, 163.1],
    [30.6, 93.7, 147.4],
    [30.4, 44.6, 61.7],
    [14.4, 27.0, 62.3],
    [36.9, 75.8, 173.1],
    [31.0, 75.1, 146.2],
    [37.0, 86.2, 179.4],
    [32.6, 82.0, 167.0],
];

/// Level-ups across all thirteen skills in each 5-minute window of the mixed run.
const LEVELUPS_PER_5MIN: [u32; 36] = [
    12, 7, 16, 8, 7, 8, 5, 10, 9, 6, 6, 6, 5, 7, 5, 7, 5, 8, 3, 5, 6, 3, 10, 3, 4, 6, 2, 3, 3, 6,
    2, 3, 2, 1, 5, 2,
];

/// Distinct kinds of work (node kinds, recipes, fights, climbs...) in each
/// 5-minute window of the mixed run.
const DISTINCT_PER_5MIN: [u32; 36] = [
    14, 7, 9, 15, 15, 13, 6, 12, 14, 13, 15, 10, 18, 17, 16, 17, 5, 15, 11, 17, 12, 13, 13, 12, 14,
    18, 6, 9, 18, 16, 16, 19, 11, 6, 15, 17,
];

/// Every stretch of over 3 minutes on one node kind or recipe while something
/// else was available: (start minute, what, seconds).
const LONG_STRETCHES: &[(f64, &str, f64)] = &[];

/// The focused Prying run: minutes to level 5, 10 and 15 (-1: not reached).
const PRYING_FOCUS: [f64; 3] = [4.9, 9.9, 15.6];

/// Next goals met in the mixed three hours.
const GOALS_DONE: u32 = 39;

/// The run that follows the board: (goals met, swapped, longest open in
/// minutes).
const FOLLOW_GOALS: (u32, u32, f64) = (86, 47, 9.8);

/// The longest any next goal stayed open in the mixed run (minutes).
const GOAL_LONGEST_OPEN: f64 = 9.4;

// ---- The measurement ----

fn minutes_row(b: &Bot, skill: Skill) -> [f64; 3] {
    [5, 10, 15].map(|l| b.minute_of(skill, l).unwrap_or(-1.0))
}

fn shown(m: f64) -> String {
    if m < 0.0 {
        "not by 3 h".to_string()
    } else {
        format!("{m:.1}")
    }
}

fn list<T: std::fmt::Display>(v: &[T]) -> String {
    v.iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn stretch_line(s: &Stretch) -> String {
    format!(
        "min {:.1}: {} for {:.0} s{}",
        s.start / 60.0,
        s.label,
        s.secs,
        if s.had_alternative {
            " (something else was available)"
        } else {
            " (nothing else was available)"
        }
    )
}

/// Print one run's next goals: (met, swapped, longest open in minutes).
fn goal_report(name: &str, b: &Bot) -> (usize, usize, f64) {
    let waits = b.goal_waits();
    let done = b.goal_log.iter().filter(|g| g.3.is_some() && !g.4).count();
    let swapped = b.goal_log.iter().filter(|g| g.4).count();
    let longest = waits.first().map_or(0.0, |w| w.0 / 60.0);
    println!();
    println!(
        "== Next goals ({name}) ==\n{} offered, {done} met, {swapped} swapped for nearer ones; longest open {longest:.1} min",
        b.goal_log.len()
    );
    if let Some(first) = b.goal_log.first() {
        println!("  the first opened at minute {:.1}", first.2 / 60.0);
    }
    for (secs, what, at) in waits.iter().take(5) {
        println!(
            "  {what}: open {:.1} min from minute {:.1}",
            secs / 60.0,
            at / 60.0
        );
    }
    (done, swapped, longest)
}

#[test]
#[ignore = "about 2.5 minutes in release; prints the cached tables"]
fn measure_first_hours() {
    let t0 = std::time::Instant::now();
    let mixed = std::thread::spawn(|| {
        let mut b = Bot::new(Config {
            focus: None,
            secs: MIXED_SECS,
            stop_level: None,
            guide: false,
            follow_goals: false,
        });
        b.play();
        b
    });
    let focus = std::thread::spawn(|| {
        let mut b = Bot::new(Config {
            focus: Some(Skill::Prying),
            secs: FOCUS_SECS,
            stop_level: Some(15),
            guide: false,
            follow_goals: false,
        });
        b.play();
        b
    });
    // A player who follows the next-goals board.
    let follow = std::thread::spawn(|| {
        let mut b = Bot::new(Config {
            focus: None,
            secs: MIXED_SECS,
            stop_level: None,
            guide: false,
            follow_goals: true,
        });
        b.play();
        b
    });
    let b = mixed.join().expect("the mixed run");
    let g = follow.join().expect("the goal-following run");
    let f = focus.join().expect("the focused run");

    println!();
    println!("== Mixed play, 3 hours of game time (bot on real sim ticks) ==");
    println!(
        "{:<11} {:>5} {:>11} {:>11} {:>11}",
        "skill", "final", "level 5", "level 10", "level 15"
    );
    let mut rows = Vec::new();
    for &s in &SKILLS {
        let row = minutes_row(&b, s);
        println!(
            "{:<11} {:>5} {:>11} {:>11} {:>11}   (minutes)",
            s.name(),
            b.levels[skill_index(s)],
            shown(row[0]),
            shown(row[1]),
            shown(row[2])
        );
        rows.push(row);
    }
    let ups = b.levelups_per_window();
    let distinct = b.distinct_per_window();
    println!();
    println!(
        "Level-ups per 5-minute window (all skills): [{}]",
        list(&ups)
    );
    println!(
        "Distinct activities per 5-minute window:    [{}]",
        list(&distinct)
    );
    let long = b.long_stretches();
    println!();
    println!("Stretches of over 3 minutes on one node kind or recipe:");
    if long.is_empty() && b.forced.is_empty() {
        println!("  none");
    }
    for s in &long {
        println!("  {}", stretch_line(s));
    }
    for s in &b.forced {
        println!("  forced: {}", stretch_line(s));
    }
    let silent = silent_levels();
    println!();
    println!(
        "Silent level-ups (levels 2-15 with no unlock line): {} of {}",
        silent.len(),
        SKILLS.len() * 14
    );
    for &s in &SKILLS {
        let levels: Vec<u32> = silent
            .iter()
            .filter(|(k, _)| *k == s)
            .map(|(_, l)| *l)
            .collect();
        if !levels.is_empty() {
            println!("  {:<11} {}", s.name(), list(&levels));
        }
    }
    println!();
    println!("== Focused run: Prying only ==");
    let focus_row = minutes_row(&f, Skill::Prying);
    println!(
        "Prying minutes to level 5 / 10 / 15: {} / {} / {} (run ended at minute {:.1})",
        shown(focus_row[0]),
        shown(focus_row[1]),
        shown(focus_row[2]),
        f.now() / 60.0
    );
    let (done, _, longest) = goal_report("mixed, the board ignored", &b);
    let (f_done, f_swapped, f_longest) = goal_report("following the board", &g);
    println!("Following the board, minutes to level 15 (final level):");
    for &s in &SKILLS {
        println!(
            "  {:<11} {} ({})",
            s.name(),
            shown(minutes_row(&g, s)[2]),
            g.levels[skill_index(s)]
        );
    }
    println!();
    println!("Gathers by item (mixed): {:?}", b.gathers);
    println!("Deaths: {} (mixed), {} (focused)", b.deaths, f.deaths);
    println!("Measurement took {:.0} s", t0.elapsed().as_secs_f64());

    println!();
    println!("// ---- paste ----");
    println!("const MIXED: [[f64; 3]; 13] = [");
    for r in &rows {
        println!("    [{:.1}, {:.1}, {:.1}],", r[0], r[1], r[2]);
    }
    println!("];");
    println!("const LEVELUPS_PER_5MIN: [u32; 36] = [{}];", list(&ups));
    println!(
        "const DISTINCT_PER_5MIN: [u32; 36] = [{}];",
        list(&distinct)
    );
    println!("const LONG_STRETCHES: &[(f64, &str, f64)] = &[");
    for s in long.iter().filter(|s| s.had_alternative) {
        println!("    ({:.1}, {:?}, {:.0}),", s.start / 60.0, s.label, s.secs);
    }
    println!("];");
    println!(
        "const PRYING_FOCUS: [f64; 3] = [{:.1}, {:.1}, {:.1}];",
        focus_row[0], focus_row[1], focus_row[2]
    );
    println!("const GOALS_DONE: u32 = {done};");
    println!("const GOAL_LONGEST_OPEN: f64 = {longest:.1};");
    println!("const FOLLOW_GOALS: (u32, u32, f64) = ({f_done}, {f_swapped}, {f_longest:.1});");
}

// ---- Fast tests ----

/// The bot plays five minutes of real ticks without trouble: it comes out of
/// the door, works, and gains experience by the sim's own rules.
#[test]
fn the_bot_plays_five_minutes() {
    let mut b = Bot::new(Config {
        focus: None,
        secs: 300.0,
        stop_level: None,
        guide: false,
        follow_goals: false,
    });
    b.play();
    assert!(b.now() >= 300.0);
    assert!(
        !b.level_ups.is_empty(),
        "five minutes of play raised no skill"
    );
    assert!(!b.spans.is_empty(), "the bot did no work");
    assert_eq!(b.deaths, 0);
}

#[test]
fn the_cached_tables_are_well_formed() {
    assert_eq!(MIXED.len(), SKILLS.len());
    assert_eq!(LEVELUPS_PER_5MIN.len(), 36);
    assert_eq!(DISTINCT_PER_5MIN.len(), 36);
    for row in MIXED {
        // Later levels never come sooner (or are -1).
        let reached: Vec<f64> = row.iter().copied().filter(|m| *m >= 0.0).collect();
        assert!(reached.windows(2).all(|w| w[0] <= w[1]));
    }
}

/// The owner's targets for levels 1 to 15, against the cached measurement.
#[test]
fn first_hours_targets() {
    let mut failures = Vec::new();

    // 1. The focused main skill (Prying) reaches level 15 by about minute 20.
    let prying = PRYING_FOCUS[2];
    if !(15.0..=25.0).contains(&prying) {
        failures.push(format!(
            "focused Prying level 15: {} (minutes); the target is 15 to 25",
            shown(prying)
        ));
    }

    // 2. Every one of the 13 skills reaches 15 within the first 3 hours of mixed play.
    for (i, &s) in SKILLS.iter().enumerate() {
        let m = MIXED[i][2];
        if !(0.0..=180.0).contains(&m) {
            failures.push(format!(
                "{} level 15 in mixed play: {} (minutes); the target is within 180 min",
                s.name(),
                shown(m)
            ));
        }
    }

    // 3. Early level-ups pop every minute or two: within the first 15 minutes
    //    no 5-minute window has fewer than 2 level-ups.
    for (w, &n) in LEVELUPS_PER_5MIN.iter().take(3).enumerate() {
        if n < 2 {
            failures.push(format!(
                "minutes {}-{} have {n} level-ups; the target is at least 2",
                w * 5,
                w * 5 + 5
            ));
        }
    }

    // 4. No stretch of more than 3 minutes on one node or recipe while
    //    anything else is available.
    for (start, what, secs) in LONG_STRETCHES {
        failures.push(format!(
            "minute {start:.1}: {secs:.0} s on {what} with something else available; the limit is 180 s"
        ));
    }

    // 5. No silent level 2-15 in any of the 13 skills.
    let silent = silent_levels();
    if !silent.is_empty() {
        failures.push(format!(
            "{} silent level-ups (levels 2-15 with no unlock line), for example {} {}",
            silent.len(),
            silent[0].0.name(),
            silent[0].1
        ));
    }

    // 6. Next goals keep coming: none stays open longer than about 10
    //    minutes of mixed play (10.5), and plenty are met.
    if GOAL_LONGEST_OPEN > 10.5 {
        failures.push(format!(
            "a next goal stayed open {GOAL_LONGEST_OPEN:.1} min; the target is about 10"
        ));
    }
    // (The bot ignores the board, and its guide ends at about minute 36, so
    // this is a floor; a player who follows the board meets more.)
    if GOALS_DONE < 25 {
        failures.push(format!(
            "{GOALS_DONE} next goals met in 3 h; the target is at least 25"
        ));
    }

    // 7. A player who follows the board meets more goals than it swaps, and
    //    none stays open longer than about 10 minutes.
    let (met, swapped, longest) = FOLLOW_GOALS;
    if met <= swapped {
        failures.push(format!(
            "following the board: {met} goals met, {swapped} swapped; the target is more met than swapped"
        ));
    }
    if longest > 10.5 {
        failures.push(format!(
            "following the board: a goal stayed open {longest:.1} min; the target is about 10"
        ));
    }

    assert!(
        failures.is_empty(),
        "{} target(s) not met:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

// ---- The first-hour guide ----

/// The bot follows the guide step by step on real ticks.
fn guided_run() -> Bot {
    let mut b = Bot::new(Config {
        focus: None,
        secs: 60.0 * 60.0,
        stop_level: None,
        guide: true,
        follow_goals: false,
    });
    b.play_guide();
    b
}

/// The minute each guide step finished, one line per step.
fn step_minutes(b: &Bot) -> Vec<String> {
    let guide = crabsim::economy::guide::GUIDE;
    guide
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let at = b
                .step_done
                .iter()
                .find(|(k, _)| *k == i)
                .map(|(_, t)| t / 60.0);
            format!(
                "{:>2}  {:<11} {:<34} minute {}",
                i + 1,
                s.skill.name(),
                s.title,
                at.map_or("not finished".to_string(), |m| format!("{m:.1}"))
            )
        })
        .collect()
}

#[test]
#[ignore = "prints the guide's measured minutes"]
fn measure_the_guide() {
    let b = guided_run();
    println!();
    for line in step_minutes(&b) {
        println!("{line}");
    }
    println!("levels: {:?}", b.levels);
    println!("ended at minute {:.1}, deaths {}", b.now() / 60.0, b.deaths);
}

/// A brand-new crab that follows the guide finishes the whole chain inside an
/// hour of real ticks, and has gained a level in all thirteen skills by then.
#[test]
fn a_crab_following_the_guide_finishes_it_within_an_hour() {
    let b = guided_run();
    let steps = crabsim::economy::guide::GUIDE.len();
    for line in step_minutes(&b) {
        eprintln!("{line}");
    }
    assert_eq!(
        b.guide_index(),
        steps,
        "the guide did not finish in an hour (at step {})",
        b.guide_index() + 1
    );
    assert_eq!(b.step_done.len(), steps, "every step finished in order");
    assert!(b.now() <= 60.0 * 60.0);
    for &s in &SKILLS {
        assert!(
            b.levels[skill_index(s)] >= 2,
            "{} did not gain a level by the end of the guide",
            s.name()
        );
    }
}
