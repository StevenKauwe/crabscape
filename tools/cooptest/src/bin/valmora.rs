//! Two real isolated headless browsers driving Crabscape's integrated travel UI.
//! Build crabscape with valmora-loopback-test, bindgen into a fresh copy of web/.
//! Run: cargo run -p cooptest --bin valmora -- WEB_DIR BROWSER_NATIVE_PROBE
//! The probe is built with dragon-peer/loopback-test. No public signaling, STUN,
//! microphone, transcription, personal browser profile or legacy Dragon saves.
//! Fixture source slots are imported only through the game's actual buttons.
//! Crabscape's normal saved-lobby/invite route supplies the real current LINK;
//! no manual Valmora room or writable game-state hook is used.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use chrome::{Chrome, Options, Result};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Signaling(Child);
impl Drop for Signaling {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
const NETWORK: &str = "JSON.parse(document.getElementById('crabscape')?.getAttribute('data-valmora-network')||'null')";
const STAGE: &str = "document.getElementById('crabscape')?.getAttribute('data-valmora')";
const VIEW: &str =
    "JSON.parse(document.getElementById('crabscape')?.getAttribute('data-valmora-view')||'null')";
const DISPLAY: &str = "JSON.parse(document.getElementById('crabscape')?.getAttribute('data-valmora-display')||'null')";
const ROOM: &str = "synthetic";
const PRIVACY: &str = r#"
window.__mediaRequests=0; window.__externalSockets=[]; window.__localSockets=[];
window.__rtcPeers=[]; window.__strippedIceServers=0;
if(navigator.mediaDevices) navigator.mediaDevices.getUserMedia=()=>{window.__mediaRequests++;return Promise.reject(new Error('Test forbids microphone'));};
const NativeWebSocket=window.WebSocket;
window.WebSocket=class extends NativeWebSocket {constructor(url,...args){const u=new URL(String(url));if(u.protocol!=='ws:'||u.hostname!=='127.0.0.1'||!u.port){window.__externalSockets.push(String(url));throw Error('Test forbids external signaling');}super(url,...args);window.__localSockets.push(String(url));}};
// Crabscape's regular transport (and idle voice negotiation) normally supplies
// public STUN. Change only the browser transport environment, before construction.
// All signaling, data channels, gameplay and UI still run their real code paths.
const hostOnly=config=>{window.__strippedIceServers+=(config?.iceServers||[]).length;return {...config,iceServers:[]};};
const NativePeer=window.RTCPeerConnection;
window.RTCPeerConnection=class extends NativePeer {
  constructor(config,...args){super(hostOnly(config),...args);window.__rtcPeers.push(this);}
  setConfiguration(config){return super.setConfiguration(hostOnly(config));}
};
"#;
fn wait(b: &mut Chrome, expression: &str, seconds: u64) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(error) = b.trouble() {
            return Err(format!("{}: {error}", b.name));
        }
        if b.eval(expression)? == true {
            return Ok(());
        }
        if Instant::now() > until {
            let stage = b.eval(STAGE)?;
            let network = b.eval(NETWORK)?;
            let view = b.eval(VIEW)?;
            let coop = b.attr("data-coop");
            return Err(format!(
                "{} timeout {expression}; stage={stage} network={network} view={view} coop={coop:?}",
                b.name
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn stage(b: &mut Chrome, want: &str) -> Result<()> {
    wait(b, &format!("{STAGE} === {want:?}"), 40)
}
fn button(b: &mut Chrome, name: &str) -> Result<()> {
    let attribute = format!("data-valmora-button-{name}");
    let expression = format!(
        "JSON.parse(document.getElementById('crabscape')?.getAttribute({attribute:?})||'null')"
    );
    // A prior panel can leave a diagnostic attribute behind. Removing that DOM
    // marker requires the currently drawn real widget to publish fresh geometry;
    // it does not change an egui response or any authoritative game state.
    for _ in 0..24 {
        b.eval(&format!(
            "document.getElementById('crabscape')?.removeAttribute({attribute:?})"
        ))?;
        wait(b, &format!("Array.isArray({expression})"), 15)?;
        let point = b.eval(&expression)?;
        let x = point[0].as_f64().ok_or("button x")?;
        let y = point[1].as_f64().ok_or("button y")?;
        let view = b.eval(VIEW)?;
        let context = view["context_rect"].as_array();
        let top = context.and_then(|r| r[1].as_f64()).map_or(2., |y| y + 32.);
        let bottom = context.and_then(|r| r[3].as_f64()).map_or(798., |y| y - 8.);
        // Votes are in a separate global window, not the contextual scroll area.
        let (top, bottom) = if name.starts_with("vote-") {
            (2., 798.)
        } else {
            (top, bottom)
        };
        if (2. ..1198.).contains(&x) && (top..bottom).contains(&y) {
            return b.canvas_click(x, y);
        }
        let scroll_x = context
            .and_then(|r| Some((r[0].as_f64()? + r[2].as_f64()?) * 0.5))
            .ok_or_else(|| format!("button {name} is outside the viewport, no scroll panel"))?;
        // Scroll the actual visible context window, never alter a game variable.
        b.canvas_scroll(
            scroll_x,
            bottom - 20.,
            if y >= bottom { 420. } else { -420. },
        )?;
    }
    Err(format!("button {name} stayed outside viewport"))
}
fn button_stage(b: &mut Chrome, name: &str, want: &str) -> Result<()> {
    // One gesture, then an assertion. Repeating a purchase/readiness gesture
    // could mask a missed interaction or accidentally undo a successful one.
    button(b, name)?;
    stage(b, want)
}
fn panel(b: &mut Chrome, want: Option<&str>) -> Result<()> {
    let want = serde_json::to_string(&want).map_err(|e| e.to_string())?;
    wait(
        b,
        &format!("({VIEW}) != null && ({VIEW}).panel === {want}"),
        20,
    )
}
fn close_panel(b: &mut Chrome) -> Result<()> {
    let view = b.eval(VIEW)?;
    let rect = view["context_rect"]
        .as_array()
        .ok_or("no contextual window to close")?;
    // egui's noncollapsible title bar places the real X inside the top-right
    // frame margin. Derive its position from this frame, never an old screen.
    let x = rect[2].as_f64().ok_or("context right")? - 16.;
    let y = rect[1].as_f64().ok_or("context top")? + 17.;
    b.canvas_click(x, y)?;
    panel(b, None)
}
fn visit(b: &mut Chrome, place: &str, want: &str) -> Result<()> {
    let label = match place {
        "dragon" => "Your dragon",
        "workshop" => "Roost workshop",
        _ => return Err(format!("unknown fixture destination {place}")),
    };
    panel(b, None)?;
    if b.eval(&format!("({VIEW})?.nearby === {label:?}"))? != true {
        button(b, &format!("landing-{place}"))?;
        wait(b, &format!("({VIEW})?.nearby === {label:?}"), 15)?;
    }
    button(b, &format!("interact-{place}"))?;
    panel(b, Some(want))
}
fn fixture(rider: usize) -> serde_json::Value {
    serde_json::json!({"v":3,"xpv":2,"rider":if rider==0 {"alexa"}else{"tavian"},"rid":format!("synthetic{rider}"),"seenIntro":true,"earned":1000,"spent":0,"briefed":["m1"],"cleared":["m1"],"build":{"name":if rider==0 {"Source Alice"}else{"Source Bobby"},"dname":if rider==0 {"Ember"}else{"Spark"},"path":"forge","dkind":"flame"},"party":[{"id":"bond","bonded":true,"xp":0}]})
}
fn prepare_castle(b: &mut Chrome, base: &str, introducer: &str, rider: usize) -> Result<()> {
    b.protocol_viewport(1200, 800)?;
    b.install_script(PRIVACY)?;
    b.go(&format!("{base}/__fixture_setup"))?;
    wait(b, "document.title==='Save fixture'", 10)?;
    if b.eval("(()=>{const c=document.createElement('canvas').getContext('2d');c.translate(12,34);const before=c.getTransform().toString();c.translate(NaN,NaN);return c.getTransform().toString()===before;})()")? != true {
        return Err("Canvas must ignore nonfinite translation".into());
    }
    let key = if rider == 0 {
        "vdf-save-alexa"
    } else {
        "vdf-save-tavian"
    };
    let original = fixture(rider).to_string();
    let name = format!("{} Crab", b.name);
    let setup = format!(
        "(()=>{{sessionStorage.setItem('synthetic-original-key',{key:?});sessionStorage.setItem('synthetic-original-bytes',{original:?});localStorage.setItem({key:?},{original:?});localStorage.setItem('crabscape.save',JSON.stringify({{version:5,x:2300,y:7100}}));localStorage.setItem('crabscape-name',{name:?});if({rider}===0)localStorage.setItem('crabscape-lobby',{ROOM:?});localStorage.setItem('vdf-music','false');localStorage.setItem('vdf-narrate','false');return localStorage.getItem({key:?})==={original:?};}})()"
    );
    if b.eval(&setup)? != true {
        return Err("synthetic import fixture failed".into());
    }
    b.go(&format!(
        "{base}/index.html?boring&zoom=1&join={ROOM}&introducer={introducer}&valmora_introducer={introducer}"
    ))?;
    wait(
        b,
        "(()=>{const c=document.getElementById('crabscape')?.getAttribute('data-crab')?.split(' ');return c?.length===3&&Math.abs(Number(c[0])-2300)<=100&&Math.abs(Number(c[1])-7100)<35&&c[2]==='1';})()",
        120,
    )?;
    wait(
        b,
        &format!("document.getElementById('crabscape')?.getAttribute('data-name')==={name:?}"),
        10,
    )?;
    stage(b, "castle")
}
fn enter_and_import(b: &mut Chrome, rider: usize) -> Result<()> {
    let crab = b.attr("data-crab").ok_or("missing source crab position")?;
    b.eval(&format!(
        "sessionStorage.setItem('synthetic-roof-position',{crab:?})"
    ))?;
    b.hold_key("KeyV", 0.2)?;
    stage(b, "menu")?;
    visit(b, "dragon", "Companion")?;
    // Integrated setup skips the source human-rider page and begins with dragon.
    button_stage(b, &format!("companion-{rider}"), "builder-1")?;
    button(b, "import-expand")?;
    button_stage(b, "import-browser", "party")?;
    panel(b, None)?;
    wait(b, &format!("({NETWORK})?.id != null"), 25)?;
    let name = format!("{} Crab", b.name);
    wait(
        b,
        &format!("({VIEW})?.party_code==={ROOM:?}&&({VIEW})?.crab_name==={name:?}"),
        10,
    )?;
    verify_privacy(b)?;
    verify_source_crab(b)?;
    println!(
        "PASS {} ferry, explicit source-slot import and inherited Crabscape identity/party",
        b.name
    );
    Ok(())
}
fn verify_source_crab(b: &mut Chrome) -> Result<()> {
    if b.eval("document.getElementById('crabscape')?.getAttribute('data-crab')===sessionStorage.getItem('synthetic-roof-position')")? != true {
        return Err(format!("{} travel moved the authoritative castle crab", b.name));
    }
    Ok(())
}
fn verify_privacy(b: &mut Chrome) -> Result<()> {
    if b.eval("localStorage.getItem(sessionStorage.getItem('synthetic-original-key'))===sessionStorage.getItem('synthetic-original-bytes')")? != true {return Err(format!("{} changed original synthetic save bytes",b.name));}

    if b.eval("window.__mediaRequests===0&&window.__externalSockets.length===0&&window.__rtcPeers.every(p=>p.getConfiguration().iceServers.length===0)")? != true {
        return Err(format!(
            "{} made forbidden microphone/external socket request",
            b.name
        ));
    }
    if b.eval("window.__localSockets.some(u=>new URL(u).pathname.startsWith('/crabscape-live-'))&&window.__localSockets.some(u=>new URL(u).pathname.startsWith('/crabscape-dragon-'))")? != true {
        return Err(format!("{} did not use both real local party transports", b.name));
    }
    Ok(())
}
fn capture(b: &mut Chrome, profiles: &Path, name: &str) -> Result<()> {
    let evidence = b.eval(&format!(
        "({{stage:{STAGE},view:{VIEW},network:{NETWORK},display:{DISPLAY},crab:document.getElementById('crabscape')?.getAttribute('data-crab'),roof:sessionStorage.getItem('synthetic-roof-position'),coop:document.getElementById('crabscape')?.getAttribute('data-coop'),mediaRequests:window.__mediaRequests,externalSockets:window.__externalSockets,localSockets:window.__localSockets,iceServers:window.__rtcPeers?.map(p=>p.getConfiguration().iceServers),sourceUnchanged:localStorage.getItem(sessionStorage.getItem('synthetic-original-key'))===sessionStorage.getItem('synthetic-original-bytes')}})"
    ))?;
    std::fs::write(
        profiles.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&evidence).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    b.screenshot(&profiles.join(format!("{name}.png")))
}
fn verify_remote(b: &mut Chrome, other: &mut Chrome, scene: &str) -> Result<()> {
    wait(
        b,
        &format!(
            "({DISPLAY})?.peers?.length===1&&({DISPLAY}).peers[0].bound&&({DISPLAY}).peers[0].avatar_ready&&({DISPLAY}).peers[0].scene==={scene:?}"
        ),
        20,
    )?;
    let shown = b.eval(DISPLAY)?;
    let other_display = other.eval(DISPLAY)?;
    let other_network = other.eval(NETWORK)?;
    let remote = &shown["peers"][0];
    if remote["crab_connection"] != other_display["local_crab_connection"]
        || remote["dragon"] != other_network["id"]
        || remote["name"] != format!("{} Crab", other.name)
    {
        return Err(format!(
            "{} remote identity mismatch: {shown} / {other_display} / {other_network}",
            b.name
        ));
    }
    Ok(())
}
/// Optional short cadence observation, not a physical-phone performance claim.
/// Two real isolated browser processes share this Mac; no CPU throttling.
fn phone_cadence(b: &mut Chrome, other: &mut Chrome, scene: &str, profiles: &Path) -> Result<()> {
    if std::env::var_os("CRABSCAPE_VALMORA_CADENCE").is_none() {
        return Ok(());
    }
    let mut load = String::new();
    for tool in ["cargo", "rustc", "wasm-bindgen"] {
        let status = Command::new("pgrep")
            .args(["-x", tool])
            .stdout(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            println!("SKIP {scene} cadence: active {tool} job; no jobs stopped or delayed");
            return Ok(());
        }
        if status.code() != Some(1) {
            return Err("cannot verify absence of build jobs for cadence sample".into());
        }
    }
    for (tool, args) in [("uptime", vec![]), ("top", vec!["-l", "1", "-n", "0"])] {
        let output = Command::new(tool)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!("cannot inspect OS load with {tool}"));
        }
        load.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    std::fs::write(profiles.join(format!("cadence-{scene}-load.txt")), load)
        .map_err(|e| e.to_string())?;
    b.responsive_viewport(390, 844)?;
    b.touch_screen(true)?;
    wait(b, "innerWidth===390&&innerHeight===844", 10)?;
    std::thread::sleep(Duration::from_secs(2));
    verify_remote(b, other, scene)?;
    capture(b, profiles, &format!("phone-{scene}-before"))?;
    let result = b.eval(&format!(
        r#"new Promise(done=>{{
            const deltas=[];let start=null,last=null,valid=0,frames=0;
            const tick=t=>{{
                if(start===null) start=t;
                if(last!==null&&deltas.length<4096) deltas.push(t-last);
                last=t;frames++;
                const p=({DISPLAY})?.peers?.[0];
                if(p?.bound&&p?.avatar_ready&&p?.scene==={scene:?}) valid++;
                if(t-start<10000) return requestAnimationFrame(tick);
                const sorted=[...deltas].sort((a,b)=>a-b),q=n=>sorted[Math.floor((sorted.length-1)*n)];
                done({{duration_ms:t-start,frames,valid_remote_frames:valid,samples:deltas.length,
                    mean_ms:deltas.reduce((a,b)=>a+b,0)/deltas.length,
                    median_ms:q(.5),p95_ms:q(.95),p99_ms:q(.99),worst_ms:sorted.at(-1),
                    over_25_ms:deltas.filter(x=>x>25).length,over_50_ms:deltas.filter(x=>x>50).length,
                    width:innerWidth,height:innerHeight,dpr:devicePixelRatio,visibility:document.visibilityState,
                    deltas_ms:deltas}});
            }};requestAnimationFrame(tick);
        }})"#
    ))?;
    std::fs::write(
        profiles.join(format!("cadence-{scene}.json")),
        serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    capture(b, profiles, &format!("phone-{scene}-after"))?;
    verify_remote(b, other, scene)?;
    verify_privacy(b)?;
    verify_source_crab(b)?;
    if result["valid_remote_frames"] != result["frames"] {
        return Err(format!(
            "remote crab disappeared during {scene} cadence sample"
        ));
    }
    println!(
        "CADENCE {scene} 390x844, two browsers on one Mac: {} samples, median {}ms, p95 {}ms, worst {}ms, >25ms {}",
        result["samples"],
        result["median_ms"],
        result["p95_ms"],
        result["worst_ms"],
        result["over_25_ms"]
    );
    b.touch_screen(false)?;
    b.protocol_viewport(1200, 800)?;
    wait(b, "innerWidth===1200&&innerHeight===800", 10)
}
fn run(web: &Path, probe: &Path, profiles: &Path) -> Result<()> {
    let mut server = Signaling(
        Command::new(probe)
            .arg("--signaling-only")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let mut url = String::new();
    BufReader::new(server.0.stdout.take().ok_or("signaling stdout")?)
        .read_line(&mut url)
        .map_err(|e| e.to_string())?;
    let url = url.trim();
    if !url.starts_with("ws://127.0.0.1:") {
        return Err(format!("invalid signaling endpoint {url}"));
    }
    let base = serve::start(web, 1200, 800).map_err(|e| e.to_string())?;
    let options = || Options {
        fake_mic: false,
        slow: 1.,
    };
    let mut a = Chrome::start_without_clipboard("Alice", &profiles.join("a"), options())?;
    let mut b = Chrome::start_without_clipboard("Bobby", &profiles.join("b"), options())?;
    let result = (|| {
        prepare_castle(&mut a, &base, url, 0)?;
        prepare_castle(&mut b, &base, url, 1)?;
        wait(
            &mut a,
            "document.getElementById('crabscape')?.getAttribute('data-world')==='host'",
            30,
        )?;
        wait(
            &mut b,
            "document.getElementById('crabscape')?.getAttribute('data-world')==='guest'",
            30,
        )?;
        for browser in [&mut a, &mut b] {
            wait(
                browser,
                "document.getElementById('crabscape')?.getAttribute('data-coop')?.startsWith('Playing with 1 friend')===true",
                30,
            )?;
        }
        println!("PASS real Crabscape invite/LINK, local host and guest together before boarding");
        enter_and_import(&mut a, 0)?;
        enter_and_import(&mut b, 1)?;
        for browser in [&mut a, &mut b] {
            wait(browser, &format!("({NETWORK})?.peers===1"), 30)?;
            verify_privacy(browser)?;
            verify_source_crab(browser)?;
        }
        verify_remote(&mut a, &mut b, "landing")?;
        verify_remote(&mut b, &mut a, "landing")?;
        println!(
            "PASS reciprocal current connections project both actual remote crab rigs/names on landing"
        );
        if std::env::var_os("CRABSCAPE_VALMORA_CADENCE").is_some() {
            // Separate the actual remote crab from Alice with ordinary input,
            // keeping both inside Alice's later narrow viewport.
            b.hold_key("KeyD", 0.45)?;
            phone_cadence(&mut a, &mut b, "landing", profiles)?;
        }
        // Use real walking/interaction to return, never a state hook. The peer
        // stays connected at the castle but its projected travel crab clears.
        for _ in 0..10 {
            if b.eval(&format!("({VIEW})?.nearby==='Castle ferry'"))? == true {
                break;
            }
            b.hold_key("KeyA", 0.25)?;
        }
        wait(&mut b, &format!("({VIEW})?.nearby==='Castle ferry'"), 5)?;
        button(&mut b, "interact-other")?;
        stage(&mut b, "castle")?;
        wait(
            &mut a,
            &format!(
                "({DISPLAY})?.peers?.length===1&&({DISPLAY}).peers[0].scene==='castle'&&!({DISPLAY}).peers[0].avatar_ready"
            ),
            15,
        )?;
        verify_source_crab(&mut b)?;
        capture(&mut a, profiles, "returned-partner-a")?;
        b.hold_key("KeyV", 0.2)?;
        stage(&mut b, "party")?;
        verify_remote(&mut a, &mut b, "landing")?;
        verify_remote(&mut b, &mut a, "landing")?;
        println!(
            "PASS ferry return clears remote projection; reboarding restores the same live crab"
        );
        // Only Alice opens the workshop. Bobby must see the global purchase
        // prompt while standing on the landing, without an old Party/Roost menu.
        visit(&mut a, "workshop", "Roost")?;
        stage(&mut a, "roost")?;
        panel(&mut b, None)?;
        let expected_x = a.eval(VIEW)?["x"].as_f64().ok_or("landing x")?;
        wait(
            &mut b,
            &format!("Math.abs(({DISPLAY})?.peers?.[0]?.x-{expected_x})<1"),
            15,
        )?;
        verify_source_crab(&mut a)?;
        verify_source_crab(&mut b)?;
        button(&mut a, "craft-armor")?;
        for browser in [&mut a, &mut b] {
            wait(browser, &format!("({NETWORK})?.vote==='voting'"), 20)?;
        }
        button(&mut b, "vote-yes")?;
        for browser in [&mut a, &mut b] {
            wait(
                browser,
                &format!("({NETWORK})?.vote==='passed' && ({NETWORK})?.teamarmor===1"),
                25,
            )?;
        }
        let purchase_a = a.eval(NETWORK)?;
        let purchase_b = b.eval(NETWORK)?;
        if purchase_a["localspent"].as_f64() != Some(30.)
            || purchase_a["localarmor"].as_f64() != Some(1.)
            || purchase_b["localspent"].as_f64() != Some(0.)
            || purchase_b["localarmor"].as_f64() != Some(0.)
        {
            return Err(format!(
                "purchase accounting mismatch {purchase_a} / {purchase_b}"
            ));
        }
        panel(&mut b, None)?;
        println!(
            "PASS workshop purchase reaches landing partner; real Yes/grant, exactly one spender"
        );
        capture(&mut a, profiles, "purchase-a")?;
        capture(&mut b, profiles, "purchase-b")?;
        close_panel(&mut a)?;
        for browser in [&mut a, &mut b] {
            visit(browser, "dragon", "Flight")?;
            stage(browser, "party")?;
            verify_source_crab(browser)?;
        }
        button(&mut a, "suggest-route")?;
        for browser in [&mut a, &mut b] {
            wait(browser, &format!("({NETWORK})?.party_mission==='m1'"), 15)?;
        }
        // Suggesting a route readies Alice, but Bobby has not agreed. Observe
        // beyond the three-second launch interval before exercising Ready.
        std::thread::sleep(Duration::from_secs(4));
        let waiting_a = a.eval(NETWORK)?;
        let waiting_b = b.eval(NETWORK)?;
        if waiting_a["state"] != "party"
            || waiting_b["state"] != "party"
            || waiting_a["party_ready"] != "m1"
            || !waiting_b["party_ready"].is_null()
            || waiting_a["party_countdown"].as_f64() != Some(0.)
            || waiting_b["party_countdown"].as_f64() != Some(0.)
        {
            return Err(format!(
                "party launched/countdown without partner readiness: {waiting_a} / {waiting_b}"
            ));
        }
        println!("PASS flight board waits for the unready Crabscape partner");
        button(&mut b, "ready")?;
        for browser in [&mut a, &mut b] {
            stage(browser, "play")?;
            wait(
                browser,
                &format!("({NETWORK})?.ghosts===1 && ({NETWORK})?.run?.length>0"),
                25,
            )?;
        }
        verify_remote(&mut a, &mut b, "flight")?;
        verify_remote(&mut b, &mut a, "flight")?;
        println!(
            "PASS flight partner avatar/name resolves to the same current Crabscape connections"
        );
        let aa = a.eval(NETWORK)?;
        let bb = b.eval(NETWORK)?;
        if aa["id"] == bb["id"]
            || aa["rider"] == bb["rider"]
            || aa["mission"] != "m1"
            || bb["mission"] != "m1"
            || aa["run"] != bb["run"]
            || aa["guest"] == bb["guest"]
        {
            return Err(format!("shared launch mismatch: {aa} / {bb}"));
        }
        println!("PASS distinct riders, Ready countdown, matching mission/run and single host");
        phone_cadence(&mut a, &mut b, "flight", profiles)?;
        capture(&mut a, profiles, "shared-flight-a")?;
        capture(&mut b, profiles, "shared-flight-b")?;
        let (host, guest) = if aa["guest"] == false {
            (&mut a, &mut b)
        } else {
            (&mut b, &mut a)
        };
        let before = guest.eval(NETWORK)?;
        guest.hold_key("ArrowRight", 0.5)?;
        let after = guest.eval(NETWORK)?;
        if before["x"] == after["x"] {
            return Err("guest movement input had no effect".into());
        }
        verify_privacy(host)?;
        verify_privacy(guest)?;
        verify_source_crab(host)?;
        verify_source_crab(guest)?;
        let epoch = after["epoch"].as_u64().ok_or("guest epoch")?;
        host.go("about:blank")?;
        wait(
            guest,
            &format!("({NETWORK})?.guest===false && ({NETWORK})?.epoch>{epoch}"),
            30,
        )?;
        wait(guest, &format!("({DISPLAY})?.peers?.length===0"), 15)?;
        let takeover = guest.eval(NETWORK)?;
        if takeover["run"] != after["run"] || takeover["mission"] != "m1" {
            return Err(format!("takeover reset mission: {takeover}"));
        }
        verify_privacy(guest)?;
        println!("PASS live guest input and host-loss takeover without resetting shared run");
        capture(guest, profiles, "host-loss-takeover")?;
        Ok(())
    })();
    if result.is_err() {
        for (browser, name) in [(&mut a, "a"), (&mut b, "b")] {
            let _ = capture(browser, profiles, &format!("failure-{name}"));
        }
    }
    result
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: valmora WEB_DIR BROWSER_NATIVE_PROBE");
        std::process::exit(2)
    }
    let profiles = std::env::temp_dir().join(format!("valmora-two-browser-{}", std::process::id()));
    std::fs::create_dir(&profiles).expect("fresh synthetic profile directory");
    match run(
        &PathBuf::from(&args[0]),
        &PathBuf::from(&args[1]),
        &profiles,
    ) {
        Ok(()) => println!(
            "PASS integrated Crabscape/Valmora two-browser journey; artifacts {}",
            profiles.display()
        ),
        Err(error) => {
            eprintln!(
                "FAIL {error}; synthetic failure artifacts {}",
                profiles.display()
            );
            std::process::exit(1)
        }
    }
}
