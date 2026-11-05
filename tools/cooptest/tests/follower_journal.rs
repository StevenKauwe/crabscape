//! Opt-in slow-browser stress case for a shared follower's bounded input journal.
use serde_json::Value;
use std::process::Command;

fn tick_span(trace: &Value) -> Option<u64> {
    let ticks: Vec<_> = ["states", "stalls", "gaps"]
        .into_iter()
        .filter_map(|key| trace[key].as_array())
        .flatten()
        .filter_map(|sample| sample["tick"].as_str()?.parse::<u64>().ok())
        .collect();
    ticks.iter().max()?.checked_sub(*ticks.iter().min()?)
}

fn compactions(trace: &Value, queued: bool) -> usize {
    let name = if queued {
        "cell-journal-compact-queued"
    } else {
        "cell-journal-compact"
    };
    trace["measures"].as_array().map_or(0, |spans| {
        spans.iter().filter(|span| span["name"] == name).count()
    })
}

#[test]
fn sparse_or_short_samples_do_not_claim_a_full_journal_window() {
    assert_eq!(tick_span(&serde_json::json!({})), None);
    let trace = serde_json::json!({
        "states":[{"tick":"100"}],
        "gaps":[{"tick":"500"}],
        "stalls":[{"tick":"7301"}],
        "measures":[{"name":"cell-journal-compact"},{"name":"cell-journal-compact-queued"},{"name":"cell-split-grant"}]
    });
    assert_eq!(tick_span(&trace), Some(7201));
    assert_eq!(compactions(&trace, false), 1);
    assert_eq!(compactions(&trace, true), 1);
}

#[test]
#[ignore = "needs local web build, introducer and Chrome; see docs/performance/follower-journal.md"]
fn lagging_shared_follower_survives_beyond_its_journal_limit() {
    let web = std::env::var("CRAB_FOLLOWER_WEB").expect("set CRAB_FOLLOWER_WEB");
    let introducer =
        std::env::var("CRAB_FOLLOWER_INTRODUCER").expect("set CRAB_FOLLOWER_INTRODUCER");
    let output = Command::new(env!("CARGO_BIN_EXE_cooptest"))
        .env_remove("CRABSCAPE_PROFILE_GPU")
        .env("CRABSCAPE_FRAME_PROFILE", "70")
        .env_remove("CRABSCAPE_PROFILE_WALK")
        .args([
            "--web",
            &web,
            "--introducer",
            &introducer,
            "--only",
            "cells-session",
            "--repeat",
            "1",
            "--slow",
            "4",
        ])
        .output()
        .expect("run bounded slow-browser stress case");
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    assert!(
        output.status.success(),
        "browser traversal or native replay failed"
    );
    assert!(
        log.contains("both cellular actors move, dig and use codes; identical full state")
            && log.contains("complete browser replay reproduces native state"),
        "missing confirmation of identical host/guest state and its native replay validation",
    );
    let mut traces = Vec::new();
    for side in ["host", "guest"] {
        let prefix = format!("frame-profile {side}: ");
        let path = log
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .expect("missing host or guest frame capture");
        let trace: Value = serde_json::from_slice(&std::fs::read(path).expect("read frame trace"))
            .expect("valid frame trace");
        println!(
            "{side}: observed tick span {:?}, compactions {}, queued compactions {}, trace {path}",
            tick_span(&trace),
            compactions(&trace, false),
            compactions(&trace, true)
        );
        traces.push(trace);
    }
    let guest = &traces[1];
    let states = guest["states"].as_array().expect("guest states");
    let region = |state: &Value| {
        state["state"]
            .as_str()
            .unwrap_or("")
            .split(':')
            .nth(1)
            .unwrap_or("")
            .to_owned()
    };
    let initial_region = region(states.first().expect("initial guest state"));
    assert!(
        !initial_region.is_empty() && states.iter().all(|state| region(state) == initial_region),
        "inconclusive stress route: guest changed lease instead of retaining its journal"
    );
    assert!(
        traces[0]["states"].as_array().is_some_and(|states| {
            !states.is_empty() && states.iter().all(|state| region(state) == initial_region)
        }),
        "inconclusive stress route: host and guest did not retain the same shared lease"
    );
    assert!(
        tick_span(guest).is_some_and(|ticks| ticks > 7200),
        "inconclusive stress run: capture did not span more than the 7200-tick journal limit"
    );
    assert!(
        compactions(guest, true) >= 1,
        "inconclusive capture: no queued guest compaction span retained (profiler keeps the latest 512 spans)"
    );
}
