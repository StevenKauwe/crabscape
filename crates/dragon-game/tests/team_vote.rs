use dragon_game::team_vote::{Message, Phase, Purchase, Vote};
use std::collections::BTreeSet;
fn members() -> BTreeSet<String> {
    ["a", "b"].into_iter().map(str::to_owned).collect()
}
fn setup() -> (Vote, Vote) {
    let mut a = Vote::default();
    let mut b = Vote::default();
    a.sync("a", members(), 0.);
    b.sync("b", members(), 0.);
    (a, b)
}
fn deliver(a: &mut Vote, b: &mut Vote, now: f64) {
    for _ in 0..8 {
        let aa = a.drain();
        let bb = b.drain();
        if aa.is_empty() && bb.is_empty() {
            return;
        }
        for m in aa {
            a.receive("a", "a", m.clone(), now, |_, _| true);
            b.receive("b", "a", m, now, |_, _| true);
        }
        for m in bb {
            a.receive("a", "b", m.clone(), now, |_, _| true);
            b.receive("b", "b", m, now, |_, _| true);
        }
    }
}
#[test]
fn majority_grants_proposer_once_and_waits_receipt() {
    let (mut a, mut b) = setup();
    b.propose(Purchase::Craft("armor".into()), 30).unwrap();
    deliver(&mut a, &mut b, 1.);
    assert_eq!(a.view.as_ref().unwrap().phase, Phase::Voting);
    assert!(b.take_grant("b").is_none());
    a.cast(true).unwrap();
    deliver(&mut a, &mut b, 2.);
    let grant = b.take_grant("b").unwrap();
    assert_eq!(grant.1, Purchase::Craft("armor".into()));
    assert!(b.take_grant("b").is_none());
    assert!(a.propose(Purchase::Craft("tea".into()), 30).is_err());
    b.receipt(grant.0, true);
    deliver(&mut a, &mut b, 3.);
    assert_eq!(a.view.as_ref().unwrap().phase, Phase::Passed);
    assert!(b.take_grant("b").is_none());
}
#[test]
fn duplicates_late_votes_and_different_proposals_cannot_replace_pending() {
    let (mut a, mut b) = setup();
    b.propose(Purchase::Build("lanterns".into()), 60).unwrap();
    deliver(&mut a, &mut b, 1.);
    let original = a.view.clone();
    a.receive(
        "a",
        "b",
        Message::Request {
            sequence: 1,
            purchase: Purchase::Craft("tea".into()),
            cost: 30,
        },
        2.,
        |_, _| true,
    );
    a.receive(
        "a",
        "b",
        Message::Request {
            sequence: 2,
            purchase: Purchase::Craft("tea".into()),
            cost: 30,
        },
        2.,
        |_, _| true,
    );
    a.receive(
        "a",
        "b",
        Message::Cast {
            round: 999,
            yes: true,
        },
        2.,
        |_, _| true,
    );
    assert_eq!(a.view, original);
    a.cast(false).unwrap();
    deliver(&mut a, &mut b, 3.);
    assert_eq!(a.view.as_ref().unwrap().phase, Phase::Failed);
    a.receive(
        "a",
        "b",
        Message::Cast {
            round: 1,
            yes: true,
        },
        4.,
        |_, _| true,
    );
    assert_eq!(a.view.as_ref().unwrap().phase, Phase::Failed);
}
#[test]
fn timeout_and_disconnect_fail_closed() {
    let (mut a, mut b) = setup();
    b.propose(Purchase::Craft("tea".into()), 30).unwrap();
    deliver(&mut a, &mut b, 0.);
    a.sync("a", members(), 120.);
    assert_eq!(a.view.as_ref().unwrap().phase, Phase::Voting);
    a.sync("a", members(), 120.01);
    assert_eq!(a.view.as_ref().unwrap().phase, Phase::Expired);
    let (mut a, mut b) = setup();
    b.propose(Purchase::Craft("tea".into()), 30).unwrap();
    deliver(&mut a, &mut b, 0.);
    a.cast(true).unwrap();
    deliver(&mut a, &mut b, 1.);
    b.sync("b", BTreeSet::from(["b".into()]), 2.);
    assert!(b.blocked());
    assert!(b.take_grant("b").is_none());
    assert!(b.propose(Purchase::Craft("armor".into()), 30).is_err());
}
#[test]
fn insufficient_recomputed_funds_and_unbound_sender_never_get_grant() {
    let (mut a, _) = setup();
    let request = Message::Request {
        sequence: 1,
        purchase: Purchase::Craft("armor".into()),
        cost: 30,
    };
    a.receive("a", "intruder", request.clone(), 1., |_, _| true);
    assert!(a.view.is_none());
    a.receive("a", "b", request, 1., |_, _| false);
    assert!(a.view.is_none());
}
#[test]
fn conflicting_same_round_snapshot_and_replayed_grant_are_rejected() {
    let (mut a, mut b) = setup();
    b.propose(Purchase::Craft("armor".into()), 30).unwrap();
    deliver(&mut a, &mut b, 1.);
    let mut forged = b.view.clone().unwrap();
    forged.revision += 50;
    forged.purchase = Purchase::Build("star".into());
    b.receive("b", "a", Message::Snapshot { view: forged }, 2., |_, _| {
        true
    });
    assert_eq!(
        b.view.as_ref().unwrap().purchase,
        Purchase::Craft("armor".into())
    );
    a.cast(true).unwrap();
    deliver(&mut a, &mut b, 3.);
    let granted = b.view.clone().unwrap();
    assert!(b.take_grant("b").is_some());
    b.receive("b", "a", Message::Snapshot { view: granted }, 4., |_, _| {
        true
    });
    assert!(b.take_grant("b").is_none());
}

#[test]
fn leaving_valmora_blocks_pending_or_late_grants() {
    let (mut a, mut b) = setup();
    b.propose(Purchase::Craft("armor".into()), 30).unwrap();
    deliver(&mut a, &mut b, 1.);
    a.cast(true).unwrap();
    deliver(&mut a, &mut b, 2.);
    b.suspend();
    assert!(b.blocked());
    assert!(b.take_grant("b").is_none());
    let (mut a, mut b) = setup();
    b.propose(Purchase::Craft("armor".into()), 30).unwrap();
    let request = b.drain().pop().unwrap();
    b.suspend();
    a.receive("a", "b", request, 1., |_, _| true);
    a.cast(true).unwrap();
    deliver(&mut a, &mut b, 2.);
    assert!(b.take_grant("b").is_none());
}

#[test]
#[ignore = "requires macOS JavaScriptCore; original source vote tally oracle"]
fn source_majority_matches_for_all_six_player_ballots() {
    let source = include_str!("../../../web/dragon/index.html");
    let tail = &source[source.find("function tallyVote(v)").unwrap()..];
    let function = &tail[..tail.find("function onVoteDoc(").unwrap()];
    let mut cases = Vec::new();
    let mut actual = Vec::new();
    for n in 1usize..=6 {
        for code in 0..3usize.pow((n - 1) as u32) {
            let mut encoded = code;
            let mut ballots = vec![];
            for _ in 1..n {
                ballots.push((encoded % 3) as i32 - 1);
                encoded /= 3;
            }
            cases.push(serde_json::json!({"n":n,"ballots":ballots}));
            let mut vote = Vote::default();
            vote.sync("p0", (0..n).map(|i| format!("p{i}")).collect(), 0.);
            vote.receive(
                "p0",
                "p0",
                Message::Request {
                    sequence: 1,
                    purchase: Purchase::Craft("tea".into()),
                    cost: 30,
                },
                0.,
                |_, _| true,
            );
            for (i, ballot) in ballots.iter().enumerate() {
                if vote.view.as_ref().unwrap().phase != Phase::Voting {
                    break;
                }
                if *ballot >= 0 {
                    vote.receive(
                        "p0",
                        &format!("p{}", i + 1),
                        Message::Cast {
                            round: 1,
                            yes: *ballot == 1,
                        },
                        1.,
                        |_, _| true,
                    );
                }
            }
            actual.push(serde_json::to_value(vote.view.unwrap().phase).unwrap());
        }
    }
    let script = format!(
        r#"let here=[];function onlineRiders(){{return here;}} {function}
const cases={};const result=cases.map(c=>{{here=Array.from({{length:c.n}},(_,i)=>i);const v={{by:0,votes:{{0:1}}}};let t=tallyVote(v);for(let i=0;i<c.ballots.length&&!t.pass&&!t.fail;i++){{if(c.ballots[i]>=0){{v.votes[i+1]=c.ballots[i]===1?1:-1;t=tallyVote(v);}}}}return t.pass?'granted':t.fail?'failed':'voting';}});print(JSON.stringify(result));"#,
        serde_json::to_string(&cases).unwrap()
    );
    let output = std::process::Command::new(
        "/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc",
    )
    .arg("-e")
    .arg(script)
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, expected);
}
