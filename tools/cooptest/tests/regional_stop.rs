//! Opt-in real stale-prefetch coverage. Missing branch execution is inconclusive.
use serde_json::Value;
use std::process::Command;

fn validate(trace: &Value) -> Result<usize, String> {
    if !trace["failure"].is_null() {
        return Err(format!(
            "sticky stop invariant failure: {}",
            trace["failure"]
        ));
    }
    let samples = trace["samples"].as_array().ok_or("missing stop samples")?;
    for sample in samples {
        let values = sample
            .as_str()
            .ok_or("non-string stop sample")?
            .split(':')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "invalid stop sample")?;
        if values.len() != 3 || values[0] != values[1] || values[2] != 0 {
            return Err(format!(
                "tick or partial frame advanced after Stop: {sample}"
            ));
        }
    }
    Ok(samples.len())
}
#[test]
fn rejects_advanced_ticks_partial_batches_and_sticky_failures() {
    assert_eq!(
        validate(&serde_json::json!({"samples":["1199:1199:0"],"failure":null})),
        Ok(1)
    );
    for sample in ["1199:1200:1", "1199:1200:0", "1199:1199:1", "bad"] {
        assert!(validate(&serde_json::json!({"samples":[sample],"failure":null})).is_err());
    }
    assert!(
        validate(&serde_json::json!({"samples":["1200:1200:0"],"failure":"1199:1200:1"})).is_err()
    );
    assert_eq!(
        validate(&serde_json::json!({"samples":[],"failure":null})),
        Ok(0)
    );
}
#[test]
#[ignore = "requires candidate web build, introducer and Chrome; no trigger means inconclusive"]
fn stale_ready_move_stops_at_its_exact_tick() {
    let web = std::env::var("CRAB_REGIONAL_STOP_WEB").expect("set CRAB_REGIONAL_STOP_WEB");
    let introducer =
        std::env::var("CRAB_REGIONAL_STOP_INTRODUCER").expect("set CRAB_REGIONAL_STOP_INTRODUCER");
    let output = Command::new(env!("CARGO_BIN_EXE_cooptest"))
        .env("CRABSCAPE_REGIONAL_STOP_PROBE", "1")
        .args([
            "--web",
            &web,
            "--introducer",
            &introducer,
            "--only",
            "cells-regions",
            "--repeat",
            "1",
            "--slow",
            "2",
        ])
        .output()
        .expect("run existing complete regional route");
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    assert!(
        output.status.success(),
        "existing regional traversal/replay assertions failed"
    );
    assert!(
        log.contains("both independent replays match native"),
        "missing independent replay confirmation"
    );
    let mut triggers = 0;
    for side in ["host", "guest"] {
        let prefix = format!("regional-stop-probe {side}: ");
        let capture = log
            .lines()
            .find_map(|l| l.strip_prefix(&prefix))
            .expect("missing browser stop capture");
        let trace: Value = serde_json::from_str(capture).expect("valid stop capture");
        if !trace.is_null() {
            triggers += validate(&trace).expect("stop invariant");
        }
    }
    assert!(
        triggers > 0,
        "inconclusive: route did not exercise a rejected ready move"
    );
    println!("validated {triggers} real stale-move stop observations");
}
