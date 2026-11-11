//! Two real isolated headless browsers driving the native Rust game's canvas UI.
//! Build crabscape with valmora-loopback-test, bindgen into a fresh copy of web/.
//! Run: cargo run -p cooptest --bin valmora -- WEB_DIR BROWSER_NATIVE_PROBE
//! The probe is built with dragon-peer/loopback-test. No public signaling, STUN,
//! microphone, transcription, personal browser profile or legacy Dragon saves.
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
const PRIVACY: &str = r#"
window.__mediaRequests=0; window.__externalSockets=[];
if(navigator.mediaDevices) navigator.mediaDevices.getUserMedia=()=>{window.__mediaRequests++;return Promise.reject(new Error('Test forbids microphone'));};
const NativeWebSocket=window.WebSocket;
window.WebSocket=class extends NativeWebSocket {constructor(url,...args){if(!String(url).startsWith('ws://127.0.0.1:')){window.__externalSockets.push(String(url));throw Error('Test forbids external signaling');}super(url,...args);}};
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
            return Err(format!(
                "{} timeout {expression}; stage={} network={}",
                b.name, stage, network
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn stage(b: &mut Chrome, want: &str) -> Result<()> {
    wait(b, &format!("{STAGE} === {want:?}"), 40)
}
fn click_stage(b: &mut Chrome, x: f64, y: f64, want: &str) -> Result<()> {
    let before = b.eval(STAGE)?;
    for _ in 0..3 {
        b.canvas_click(x, y)?;
        std::thread::sleep(Duration::from_secs(2));
        let current = b.eval(STAGE)?;
        if current.as_str() == Some(want) {
            return Ok(());
        }
        // Never repeat a coordinate against an unexpected/new screen.
        if current != before {
            break;
        }
    }
    stage(b, want)
}
fn button(b: &mut Chrome, name: &str) -> Result<()> {
    let expression = format!(
        "JSON.parse(document.getElementById('crabscape')?.getAttribute('data-valmora-button-{name}')||'null')"
    );
    wait(b, &format!("Array.isArray({expression})"), 15)?;
    let point = b.eval(&expression)?;
    b.canvas_click(
        point[0].as_f64().ok_or("button x")?,
        point[1].as_f64().ok_or("button y")?,
    )
}
fn prepare(b: &mut Chrome, base: &str, introducer: &str, rider: usize) -> Result<()> {
    b.protocol_viewport(1200, 800)?;
    b.install_script(PRIVACY)?;
    b.go(&format!("{base}/__fixture_setup"))?;
    wait(b, "document.title==='Save fixture'", 10)?;
    if b.eval("(()=>{const c=document.createElement('canvas').getContext('2d');c.translate(12,34);const before=c.getTransform().toString();c.translate(NaN,NaN);return c.getTransform().toString()===before;})()")? != true {
        return Err("Canvas must ignore nonfinite translation".into());
    }
    if b.eval(
        "(localStorage.setItem('crabscape.save',JSON.stringify({version:5,x:2300,y:7100})),true)",
    )? != true
    {
        return Err("synthetic roof fixture failed".into());
    }
    b.go(&format!(
        "{base}/index.html?boring&zoom=1&valmora_introducer={introducer}"
    ))?;
    wait(
        b,
        "document.querySelector('canvas')?.getAttribute('data-crab')?.startsWith('2300')===true",
        120,
    )?;
    std::thread::sleep(Duration::from_secs(2));
    b.hold_key("KeyV", 0.2)?;
    stage(b, "menu")?;
    click_stage(b, 25., 78. + 39. * rider as f64, "builder-0")?;
    b.canvas_click(90., 365.)?;
    b.insert_text(if rider == 0 { "Alice" } else { "Bobby" })?;
    b.canvas_click(30., 642.)?;
    click_stage(b, 65., 720., "builder-1")?;
    b.canvas_click(90., 365.)?;
    b.insert_text(if rider == 0 { "Ember" } else { "Spark" })?;
    b.canvas_click(90., 413.)?;
    click_stage(b, 65., 645., "story")?;
    click_stage(b, 38., 277., "party")?;
    b.canvas_click(100., 57.)?;
    b.insert_text("synthetic")?;
    b.canvas_click(355., 57.)?;
    wait(b, &format!("({NETWORK})?.id != null"), 25)?;
    println!("PASS {} rooftop, new rider, story and live room", b.name);
    Ok(())
}
fn verify_privacy(b: &mut Chrome) -> Result<()> {
    if b.eval("window.__mediaRequests===0&&window.__externalSockets.length===0")? != true {
        return Err(format!(
            "{} made forbidden microphone/external socket request",
            b.name
        ));
    }
    Ok(())
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
        prepare(&mut a, &base, url, 0)?;
        prepare(&mut b, &base, url, 1)?;
        for browser in [&mut a, &mut b] {
            wait(browser, &format!("({NETWORK})?.peers===1"), 30)?;
            verify_privacy(browser)?;
        }
        button(&mut a, "mission-m1")?;
        button(&mut b, "ready")?;
        for browser in [&mut a, &mut b] {
            stage(browser, "play")?;
            wait(
                browser,
                &format!("({NETWORK})?.ghosts===1 && ({NETWORK})?.run?.length>0"),
                25,
            )?;
        }
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
        a.screenshot(&profiles.join("shared-flight-a.png"))?;
        b.screenshot(&profiles.join("shared-flight-b.png"))?;
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
        let epoch = after["epoch"].as_u64().ok_or("guest epoch")?;
        host.go("about:blank")?;
        wait(
            guest,
            &format!("({NETWORK})?.guest===false && ({NETWORK})?.epoch>{epoch}"),
            30,
        )?;
        let takeover = guest.eval(NETWORK)?;
        if takeover["run"] != after["run"] || takeover["mission"] != "m1" {
            return Err(format!("takeover reset mission: {takeover}"));
        }
        verify_privacy(guest)?;
        println!("PASS live guest input and host-loss takeover without resetting shared run");
        guest.screenshot(&profiles.join("host-loss-takeover.png"))?;
        Ok(())
    })();
    if result.is_err() {
        for (browser, name) in [(&mut a, "a"), (&mut b, "b")] {
            let _ = browser.screenshot(&profiles.join(format!("failure-{name}.png")));
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
        Ok(()) => println!("PASS full native Valmora two-browser journey"),
        Err(error) => {
            eprintln!(
                "FAIL {error}; synthetic failure artifacts {}",
                profiles.display()
            );
            std::process::exit(1)
        }
    }
}
