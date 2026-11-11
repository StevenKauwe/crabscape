use dragon_game::authority::*;
#[test]
fn paused_host_does_not_trigger_takeover() {
    let mut a = Authority {
        guest: true,
        host_peer: Some("a".into()),
        host_rider: 1,
        epoch: 2,
    };
    let result = a.reconcile(
        "b",
        &[Candidate {
            peer: "a".into(),
            rider: 1,
            host: true,
            epoch: 2,
            paused: true,
        }],
        true,
    );
    assert!(result.refresh_world_time);
    assert!(!result.become_host);
    assert!(!result.reset_presence);
}
#[test]
fn silent_host_without_successor_requests_local_takeover() {
    let mut a = Authority {
        guest: true,
        host_peer: Some("a".into()),
        host_rider: 1,
        epoch: 2,
    };
    let result = a.reconcile("b", &[], true);
    assert!(result.become_host && result.reset_presence);
    // Side effects and epoch increment belong to synchronous becomeHost, not election.
    assert_eq!(a.epoch, 2);
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires system JavaScriptCore"]
fn source_js_authority_election_matches_exhaustive_candidate_combinations() {
    use serde_json::{Value, json};
    let source = include_str!("../../../web/dragon/index.html");
    let start = source.find("function reconcileSkyHost(").unwrap();
    let end = source[start..].find("// Guests glide").unwrap() + start;
    let function = &source[start..end];
    let mut cases = vec![];
    let mut expected = vec![];
    for flags in 0..512 {
        let guest = flags & 1 != 0;
        let timed = flags & 2 != 0;
        let mine_out = flags & 4 != 0;
        let mut a = Authority {
            guest,
            host_peer: Some("a".into()),
            host_rider: 1,
            epoch: 1,
        };
        let mut candidates = vec![];
        let mut ghosts = vec![];
        for (i, peer) in ["a", "c"].into_iter().enumerate() {
            let offset = 3 + i * 3;
            let host = flags & (1 << offset) != 0;
            let epoch = if flags & (1 << (offset + 1)) != 0 {
                2
            } else {
                1
            };
            let paused = flags & (1 << (offset + 2)) != 0;
            candidates.push(Candidate {
                peer: peer.into(),
                rider: i + 1,
                host,
                epoch,
                paused,
            });
            ghosts.push(json!([peer,{"ms":"m1","run":"run","rider":i+1,"msHost":host,"authority":epoch,"status":if paused{"pause"}else{"play"}}]));
        }
        if !mine_out {
            candidates.push(Candidate {
                peer: "b".into(),
                rider: 0,
                host: !guest,
                epoch: 1,
                paused: false,
            });
        }
        let result = a.reconcile("b", &candidates, timed);
        cases.push(json!({"guest":guest,"timed":timed,"out":mine_out,"ghosts":ghosts}));
        expected.push(json!({"guest":a.guest,"peer":a.host_peer,"rider":a.host_rider,"epoch":a.epoch,"refresh":result.refresh_world_time,"takeover":result.become_host,"reset":result.reset_presence}));
    }
    let script = format!(
        r#"const STATIC=true,room={{peerId:()=> 'b'}},ri=0,state='play',time=42;let mission,P,ghosts,lastSent,sendT,takeover;function becomeHost(){{takeover=true;}} {function}
const cases={cases}; print(JSON.stringify(cases.map(c=>{{mission={{def:{{id:'m1'}},run:'run',guest:c.guest,hostPeer:'a',hostRider:1,authority:1,lastWorld:0}};P={{out:c.out}};ghosts=new Map(c.ghosts);lastSent='old';sendT=2;takeover=false;reconcileSkyHost(c.timed);return {{guest:mission.guest,peer:mission.hostPeer,rider:mission.hostRider,epoch:mission.authority,refresh:mission.lastWorld===42,takeover,reset:lastSent===''}};}})));"#,
        cases = serde_json::to_string(&cases).unwrap()
    );
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child =
        Command::new("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc")
            .args(["-e", "eval(JSON.parse(readline()))"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", serde_json::to_string(&script).unwrap()).as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json!(expected), actual);
}
