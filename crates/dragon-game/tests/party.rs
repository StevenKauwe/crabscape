//! Compare the countdown directly with the unchanged browser source.
#[test]
#[ignore = "requires macOS JavaScriptCore CLI"]
fn countdown_matches_browser_for_ready_changes_and_frame_schedules() {
    let source = include_str!("../../../web/dragon/index.html");
    let start = source.find("function updatePartyCountdown(dt) {").unwrap();
    let tail = &source[start..];
    let end = tail.find("\n// Who runs the sky").unwrap();
    let frames: Vec<_> = (0..500)
        .map(|i| {
            let ready = !(70..90).contains(&i) && !(215..240).contains(&i);
            (if i % 3 == 0 { 0.05 } else { 0.016 }, ready)
        })
        .collect();
    let script = format!(
        "let state='party', partyCount=0, partyPick={{m:'m1'}}, ready=false, launches=0; const sfx={{wave(){{}},pick(){{}}}}; function allPartyReady(){{return ready}} function updateCountdown(){{}} function refreshPartyScreen(){{}} function partyLaunch(){{launches++;partyCount=0}}\n{}\nconst frames={};print(JSON.stringify(frames.map(([dt,r])=>{{ready=r;updatePartyCountdown(dt);return [partyCount,launches]}})));",
        &tail[..end],
        serde_json::to_string(&frames).unwrap()
    );
    let path = std::env::temp_dir().join(format!("dragon-party-oracle-{}.js", std::process::id()));
    std::fs::write(&path, script).unwrap();
    let output = std::process::Command::new(
        "/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc",
    )
    .arg(&path)
    .output()
    .unwrap();
    let _ = std::fs::remove_file(path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<(f64, u32)> = serde_json::from_slice(&output.stdout).unwrap();
    let mut party = dragon_game::party::Party::default();
    party.pick("m1", 1.);
    let mut launches = 0;
    for ((dt, ready), (count, n)) in frames.into_iter().zip(expected) {
        if party.tick(dt, "party", ready).is_some() {
            launches += 1;
        }
        assert!((party.countdown - count).abs() < 1e-12);
        assert_eq!(launches, n);
    }
}
