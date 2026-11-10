//! Targeted Dragon acceptance: actual headless browsers, local Matchbox signaling, no external game server.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use chrome::{Chrome, Options, Result};
use std::{
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Introducer(Child);
impl Drop for Introducer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait(browser: &mut Chrome, expression: &str, seconds: u64) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(error) = browser.trouble() {
            return Err(format!("{}: {error}", browser.name));
        }
        if browser.eval(expression)? == true {
            return Ok(());
        }
        if Instant::now() >= end {
            let state = browser.eval("({state:{...document.getElementById('sky')?.dataset}, status:document.querySelector('[data-p2p-status]')?.textContent})")?;
            return Err(format!(
                "{}: timed out: {expression}; {state}",
                browser.name
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn fixture(browser: &mut Chrome, base: &str, name: &str, earned: u32) -> Result<()> {
    browser.go(&format!("{base}/__fixture_setup"))?;
    wait(browser, "document.title === 'Save fixture'", 10)?;
    browser.eval(&format!(
        r#"localStorage.setItem('vdf-fancy','false');localStorage.setItem('vdf-save-alexa', JSON.stringify({{
        v:3,xpv:2,rider:'alexa',rid:'fixture-{name}',seenIntro:true,earned:{earned},spent:0,
        build:{{name:'{name}',dname:'Dragon{name}',path:'forge',dkind:'flame'}},
        party:[{{id:'bond',bonded:true,xp:600}}],cleared:[],briefed:['m1']
    }}))"#
    ))?;
    Ok(())
}
fn pick(browser: &mut Chrome) -> Result<()> {
    wait(
        browser,
        "document.querySelector('#riderCards button')?.textContent.includes('Fly as') === true",
        20,
    )?;
    browser.eval("document.querySelector('#riderCards button').click()")?;
    wait(
        browser,
        "document.getElementById('sky')?.dataset.dragonState === 'party'",
        15,
    )
}
fn value(browser: &mut Chrome, field: &str) -> Result<String> {
    Ok(browser
        .eval(&format!("document.getElementById('sky').dataset.{field}"))?
        .as_str()
        .unwrap_or_default()
        .to_string())
}
// Aim with real controls at an observed enemy. Random enemy heights must not decide whether CI passes.
fn attack_host(guest: &mut Chrome, host: &mut Chrome) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(55);
    guest.set_key("Space", true)?;
    loop {
        if value(host, "dragonHits")?.parse::<u64>().unwrap_or(0) > 0 {
            break;
        }
        if let Some(error) = guest.trouble() {
            return Err(error);
        }
        let data = guest.eval(
            r#"(() => { const d = document.getElementById('sky').dataset;
            return {x:+d.dragonX,y:+d.dragonY,s:+d.dragonScale,f:+d.dragonFacing,
                targets:JSON.parse(d.dragonTargets || '[]')}; })()"#,
        )?;
        let x = data["x"].as_f64().ok_or("missing guest x")?;
        let y = data["y"].as_f64().ok_or("missing guest y")?;
        let scale = data["s"].as_f64().ok_or("missing guest scale")?;
        let target = data["targets"].as_array().and_then(|targets| {
            targets.iter().min_by(|a, b| {
                let distance = |t: &serde_json::Value| {
                    (t[0].as_f64().unwrap_or(x) - x).hypot(t[1].as_f64().unwrap_or(y) - y)
                };
                distance(a).total_cmp(&distance(b))
            })
        });
        let (mut dx, dy) = if let Some(target) = target {
            let tx = target[0].as_f64().ok_or("target x")?;
            let ty = target[1].as_f64().ok_or("target y")?;
            let side = if x < tx { -1.0 } else { 1.0 };
            let mut dx = tx + side * scale * 3.0 - x;
            if dx.abs() < 8.0 && data["f"].as_f64() != Some(-side) {
                dx = -side * 9.0;
            }
            (dx, ty + scale * 0.2 - y)
        } else {
            (0.0, 0.0)
        };
        if !dx.is_finite() {
            dx = 0.0;
        }
        for (key, down) in [
            ("ArrowRight", dx > 8.0),
            ("ArrowLeft", dx < -8.0),
            ("ArrowDown", dy > 8.0),
            ("ArrowUp", dy < -8.0),
        ] {
            guest.set_key(key, down)?;
        }
        if Instant::now() >= end {
            return Err(format!(
                "guest attacks did not reach host; guest={data}; host hits={}",
                value(host, "dragonHits")?
            ));
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    for key in ["Space", "ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp"] {
        guest.set_key(key, false)?;
    }
    Ok(())
}
fn check_save(browser: &mut Chrome, name: &str) -> Result<()> {
    wait(
        browser,
        &format!(
            r#"JSON.parse(localStorage.getItem('vdf-save-alexa')).build.name === '{name}' &&
        !Object.keys(localStorage).some(k => k.startsWith('vdf-save-') && k !== 'vdf-save-alexa')"#
        ),
        5,
    )
}

// Synthetic capture only: these profiles have Chrome's fake microphone flags.
// No real microphone permission or personal browser profile is used by this test.
const VOICE_PROBE: &str = r#"(() => {
  const devices = navigator.mediaDevices;
  if (!devices) return;
  const original = devices.getUserMedia.bind(devices);
  const probe = window.voiceProbe = {requests: 0, streams: [], mode: 'normal', release: null};
  devices.getUserMedia = constraints => {
    probe.requests++;
    if (probe.mode === 'deny') return Promise.reject(new DOMException('Test denial', 'NotAllowedError'));
    return original(constraints).then(stream => {
      probe.streams.push(stream);
      if (probe.mode === 'defer') return new Promise(resolve => { probe.release = () => resolve(stream); });
      return stream;
    });
  };
})();"#;
fn voice_wait(browser: &mut Chrome, expression: &str, seconds: u64) -> Result<()> {
    wait(
        browser,
        &format!(
            "(() => {{const v=JSON.parse(document.documentElement.dataset.dragonVoice||'{{}}');return {expression};}})()"
        ),
        seconds,
    )
}
fn click(browser: &mut Chrome, selector: &str) -> Result<()> {
    browser.eval(&format!(
        "document.querySelector({selector:?}).scrollIntoView({{block:'center'}})"
    ))?;
    let point = browser.eval(&format!("(() => {{const r=document.querySelector({selector:?}).getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2];}})()"))?;
    browser.click(
        point[0].as_f64().ok_or("missing button x")?,
        point[1].as_f64().ok_or("missing button y")?,
    )
}
fn voice_pair(a: &mut Chrome, b: &mut Chrome, dir: &Path, url: &str) -> Result<()> {
    const MIC: &str = "#partyScreen [data-p2p-mic]";
    b.responsive_viewport(390, 844)?;
    for browser in [&mut *a, &mut *b] {
        voice_wait(
            browser,
            "v.mic === 'off' && v.connected === 1 && window.voiceProbe.requests === 0",
            30,
        )?;
        click(browser, "#partyScreen [data-p2p-listen]")?;
        voice_wait(browser, "v.listening === true", 10)?;
    }
    click(a, MIC)?;
    voice_wait(a, "v.mic === 'on' && window.voiceProbe.requests === 1", 15)?;
    voice_wait(b, "v.hearing === 1", 20)?;
    b.eval("(()=>{const r=document.querySelector('#partyScreen [data-p2p-volume]');r.value=0;r.dispatchEvent(new Event('input',{bubbles:true}));})()")?;
    voice_wait(
        b,
        "v.volume === 0 && v.mic === 'off' && window.voiceProbe.requests === 0",
        5,
    )?;
    voice_wait(b, "v.hearing === 0", 10)?;
    b.eval("(()=>{const r=document.querySelector('#partyScreen [data-p2p-volume]');r.value=35;r.dispatchEvent(new Event('input',{bubbles:true}));})()")?;
    voice_wait(b, "Math.abs(v.volume - 0.35) < 0.001", 5)?;
    voice_wait(b, "v.hearing === 1", 10)?;
    click(b, MIC)?;
    voice_wait(b, "v.mic === 'on'", 15)?;
    voice_wait(a, "v.hearing === 1", 20)?;
    a.screenshot(&dir.join("voice-two-way.png"))?;
    b.screenshot(&dir.join("voice-phone.png"))?;
    if b.eval("[...document.querySelectorAll('#partyScreen [data-p2p-voice] button,#partyScreen [data-p2p-voice] input')].every(e=>{const r=e.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.height>=44;})")? != true {
        return Err("phone voice controls overflow or are too small to tap".into());
    }
    for browser in [&mut *a, &mut *b] {
        click(browser, MIC)?;
        voice_wait(
            browser,
            "v.mic === 'off' && window.voiceProbe.streams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))",
            10,
        )?;
    }
    voice_wait(b, "v.hearing === 0", 10)?;
    // Mute wins even if the browser resolves an earlier permission request later.
    a.eval("window.voiceProbe.mode='defer'")?;
    click(a, MIC)?;
    voice_wait(
        a,
        "v.mic === 'asking' && typeof window.voiceProbe.release === 'function'",
        15,
    )?;
    click(a, MIC)?;
    a.eval("window.voiceProbe.release()")?;
    voice_wait(
        a,
        "v.mic === 'off' && window.voiceProbe.streams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))",
        10,
    )?;
    a.eval("window.voiceProbe.mode='deny'")?;
    click(a, MIC)?;
    voice_wait(a, "v.mic === 'off' && v.error?.includes('blocked')", 10)?;
    a.eval("window.voiceProbe.mode='normal'")?;
    click(a, MIC)?;
    voice_wait(a, "v.mic === 'on'", 15)?;
    voice_wait(b, "v.hearing === 1", 20)?;
    a.eval("window.voiceProbe.streams.at(-1).getTracks().forEach(t=>t.stop())")?;
    voice_wait(a, "v.mic === 'off' && v.error?.includes('stopped')", 10)?;
    click(a, MIC)?;
    voice_wait(a, "v.mic === 'on'", 15)?;
    // Leaving must cancel capture, clear calls, and stop every acquired track.
    click(a, "#soloBtn")?;
    voice_wait(
        a,
        "v.mic === 'off' && v.calls === 0 && window.voiceProbe.streams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))",
        10,
    )?;
    voice_wait(b, "v.calls === 0", 15)?;
    // Leave also wins when permission resolves after the engine was freed.
    a.go(url)?;
    pick(a)?;
    voice_wait(
        a,
        "v.mic === 'off' && v.connected === 1 && window.voiceProbe.requests === 0",
        30,
    )?;
    a.eval("window.voiceProbe.mode='defer'")?;
    click(a, MIC)?;
    voice_wait(
        a,
        "v.mic === 'asking' && typeof window.voiceProbe.release === 'function'",
        15,
    )?;
    click(a, "#soloBtn")?;
    a.eval("window.voiceProbe.release()")?;
    voice_wait(
        a,
        "v.mic === 'off' && v.calls === 0 && window.voiceProbe.streams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))",
        10,
    )?;
    b.protocol_viewport(1200, 800)?;
    println!(
        "PASS fake-mic two-way voice, independent volume, mute, late cancellation, denial and Leave cleanup"
    );
    Ok(())
}
fn run(dir: &Path) -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let web = root.join("web");
    let base = serve::start(&web, 390, 844).map_err(|e| e.to_string())?;
    let port = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| e.to_string())?
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    let bin = std::env::var_os("DRAGON_INTRODUCER")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tools/introducer/target/release/introducer"));
    let child = Command::new(bin)
        .env("PORT", port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let _introducer = Introducer(child);
    let until = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        if Instant::now() > until {
            return Err("introducer did not start".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut a = Chrome::start(
        "Dragon A",
        &dir.join("a"),
        Options {
            fake_mic: true,
            ..Options::default()
        },
    )?;
    let mut b = Chrome::start(
        "Dragon B",
        &dir.join("b"),
        Options {
            fake_mic: true,
            ..Options::default()
        },
    )?;
    let mut c = Chrome::start("Dragon separate room", &dir.join("c"), Options::default())?;
    a.install_script(VOICE_PROBE)?;
    b.install_script(VOICE_PROBE)?;
    fixture(&mut a, &base, "Host", 120)?;
    fixture(&mut b, &base, "Guest", 777)?;
    fixture(&mut c, &base, "Other", 444)?;
    let url = format!("{base}/dragon/?diagnostics&join=dragon23&introducer=ws://127.0.0.1:{port}");
    a.go(&url)?;
    b.go(&url)?;
    c.go(&format!(
        "{base}/dragon/?diagnostics&join=other234&introducer=ws://127.0.0.1:{port}"
    ))?;
    pick(&mut a)?;
    pick(&mut b)?;
    pick(&mut c)?;
    wait(
        &mut a,
        "document.getElementById('sky').dataset.dragonFriends === '1'",
        25,
    )?;
    wait(
        &mut b,
        "document.getElementById('sky').dataset.dragonFriends === '1'",
        25,
    )?;
    wait(
        &mut c,
        "document.documentElement.dataset.dragonLink === 'connected' && document.getElementById('sky').dataset.dragonFriends === '0'",
        10,
    )?;
    let ar = value(&mut a, "dragonRider")?;
    let br = value(&mut b, "dragonRider")?;
    if ar == br {
        return Err("duplicate device-local save slots were not isolated in the session".into());
    }
    check_save(&mut a, "Host")?;
    check_save(&mut b, "Guest")?;
    println!("PASS isolated rooms, source-bound distinct riders, original device saves");
    c.go("about:blank")?; // Stop rendering the unrelated room before the functional two-player checks.
    voice_pair(&mut a, &mut b, dir, &url)?;
    a.go(&url)?;
    pick(&mut a)?;
    voice_wait(
        &mut a,
        "v.mic === 'off' && v.connected === 1 && window.voiceProbe.requests === 0",
        30,
    )?;
    b.reload()?;
    pick(&mut b)?;
    voice_wait(
        &mut b,
        "v.mic === 'off' && v.connected === 1 && window.voiceProbe.requests === 0 && Math.abs(v.volume - 0.35) < 0.001",
        30,
    )?;
    // pick() above selects through DOM for the mission test; a real tap or
    // this explicit playback button supplies the gesture needed after reload.
    click(&mut a, "#partyScreen [data-p2p-listen]")?;
    voice_wait(&mut a, "v.listening === true", 10)?;
    click(&mut b, "#partyScreen [data-p2p-mic]")?;
    voice_wait(&mut b, "v.mic === 'on'", 15)?;
    voice_wait(&mut a, "v.hearing === 1", 20)?;
    click(&mut b, "#partyScreen [data-p2p-mic]")?;
    println!(
        "PASS reload/rejoin starts mic off, keeps volume preference, and reconnects actual audio"
    );
    a.eval("document.querySelector('[data-pmission=m1]').click()")?;
    b.eval("document.querySelector('[data-pmission=m1]').click()")?;
    wait(
        &mut a,
        "document.getElementById('sky').dataset.dragonState === 'play'",
        35,
    )?;
    wait(
        &mut b,
        "document.getElementById('sky').dataset.dragonState === 'play'",
        35,
    )?;
    let a_host = value(&mut a, "dragonRole")? == "host";
    let (host, guest, host_name, guest_name) = if a_host {
        (&mut a, &mut b, "Host", "Guest")
    } else {
        (&mut b, &mut a, "Guest", "Host")
    };
    wait(
        guest,
        "document.getElementById('sky').dataset.dragonRole === 'guest' && +document.getElementById('sky').dataset.dragonWorld > 0",
        15,
    )?;
    let mut run = value(host, "dragonRun")?;
    wait(
        guest,
        &format!("document.getElementById('sky').dataset.dragonRun === '{run}'"),
        10,
    )?;
    let x = value(guest, "dragonX")?;
    guest.set_key("ArrowRight", true)?;
    let moved = wait(
        guest,
        &format!("+document.getElementById('sky').dataset.dragonX > +{x} + 8"),
        5,
    );
    guest.set_key("ArrowRight", false)?;
    if let Err(error) = moved {
        guest.screenshot(&dir.join("movement-failed.png"))?;
        return Err(format!("guest movement did not respond x={x}: {error}, page={}", guest.eval("({data:{...document.getElementById('sky').dataset},active:document.activeElement?.tagName,visible:document.visibilityState})")?));
    }
    // A guest cannot pretend to be the host by claiming its rider number, or send authoritative worlds as itself.
    let hr = value(host, "dragonRider")?;
    let gr = value(guest, "dragonRider")?;
    guest.eval(&format!(
        r#"(async()=>{{const room=await window.valmoraP2PReady;
        await room.emit('world',{{r:+{hr},mid:'m1',run:'spoofed',done:1}});
        await room.emit('world',{{r:+{gr},mid:'m1',run:'spoofed',done:1}});return true;}})()"#
    ))?;
    std::thread::sleep(Duration::from_millis(400));
    if value(host, "dragonRun")? != run {
        return Err("forged guest world replaced the host".into());
    }
    println!("PASS shared mission, host world, guest movement, forged authority dropped");
    // Fire through the actual game controls, not through a test-only gameplay hook.
    attack_host(guest, host)?;
    println!("PASS guest attacks reach the host's monsters");
    // A second launch must not pin the host's previous run id from stale Party presence.
    for browser in [&mut *guest, &mut *host] {
        browser.eval("document.getElementById('pauseBtn').click();document.getElementById('leaveBtn').click();document.getElementById('mapPartyBtn').click()")?;
        wait(
            browser,
            "document.getElementById('sky').dataset.dragonState === 'party'",
            10,
        )?;
    }
    std::thread::sleep(Duration::from_millis(500));
    host.eval("document.querySelector('[data-pmission=m1]').click()")?;
    guest.eval("document.querySelector('[data-pmission=m1]').click()")?;
    wait(
        host,
        "document.getElementById('sky').dataset.dragonRole === 'host'",
        35,
    )?;
    wait(
        guest,
        "document.getElementById('sky').dataset.dragonRole === 'guest'",
        35,
    )?;
    let previous_run = run;
    run = value(host, "dragonRun")?;
    if run == previous_run {
        return Err("second launch reused the previous mission".into());
    }
    wait(
        guest,
        &format!("document.getElementById('sky').dataset.dragonRun === '{run}'"),
        10,
    )?;
    println!("PASS leave, Party, repeated launch with a fresh shared mission");
    guest.reload()?;
    pick(guest)?;
    wait(
        guest,
        "document.getElementById('sky').dataset.dragonFriends === '1'",
        25,
    )?;
    guest.eval("document.querySelector('[data-pmission=m1]').click()")?;
    // Ready automatically joins a friend who launched earlier, even with no local countdown yet.
    wait(
        guest,
        &format!(
            "document.getElementById('sky').dataset.dragonRole === 'guest' && document.getElementById('sky').dataset.dragonRun === '{run}'"
        ),
        35,
    )?;
    check_save(guest, guest_name)?;
    println!("PASS reload, unchanged local profile, rejoin active mission");
    // A third device joins the running mission, after proving its original room was isolated.
    c.go(&url)?;
    pick(&mut c)?;
    wait(
        &mut c,
        "document.getElementById('sky').dataset.dragonFriends === '2'",
        25,
    )?;
    c.eval("document.querySelector('[data-pmission=m1]').click()")?;
    wait(
        &mut c,
        "document.querySelector('[data-party=start]') !== null",
        10,
    )?;
    c.eval("document.querySelector('[data-party=start]').click()")?;
    wait(
        &mut c,
        &format!(
            "document.getElementById('sky').dataset.dragonRole === 'guest' && document.getElementById('sky').dataset.dragonRun === '{run}'"
        ),
        20,
    )?;
    check_save(&mut c, "Other")?;
    let epoch = value(host, "dragonEpoch")?;
    let cr = value(&mut c, "dragonRider")?;
    c.eval(&format!(r#"(async()=>{{const room=await window.valmoraP2PReady;
        await room.emit('world',{{r:+{hr},mid:'m1',run:'{run}',epoch:+{epoch},done:1}});
        await room.emit('world',{{r:+{cr},mid:'m1',run:'{run}',epoch:+{epoch},done:1}});return true;}})()"#))?;
    std::thread::sleep(Duration::from_millis(400));
    if value(guest, "dragonDone")? != "false" {
        return Err("a nonhost completed another guest's mission".into());
    }
    // A host's explicit Pause keeps its authority, even beyond the old 3.5-second timeout.
    host.eval("document.getElementById('pauseBtn').click()")?;
    std::thread::sleep(Duration::from_secs(6));
    if value(guest, "dragonRole")? != "guest" || value(&mut c, "dragonRole")? != "guest" {
        return Err("pausing the host created another authority".into());
    }
    host.eval("document.getElementById('pauseBtn').click()")?;
    println!("PASS three players and host pause/resume without split brains");
    check_save(host, host_name)?;
    let next_uid = value(host, "dragonUid")?
        .parse::<u64>()
        .map_err(|e| e.to_string())?;
    host.go("about:blank")?;
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let roles = [value(guest, "dragonRole")?, value(&mut c, "dragonRole")?];
        if roles.iter().filter(|r| r.as_str() == "host").count() == 1
            && roles.iter().filter(|r| r.as_str() == "guest").count() == 1
        {
            break;
        }
        if Instant::now() >= end {
            return Err(format!("host loss did not elect one successor: {roles:?}"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (successor, follower) = if value(guest, "dragonRole")? == "host" {
        (guest, &mut c)
    } else {
        (&mut c, guest)
    };
    wait(
        follower,
        "+document.getElementById('sky').dataset.dragonEpoch > 0",
        10,
    )?;
    wait(
        successor,
        &format!(
            "+document.getElementById('sky').dataset.dragonUid > {next_uid} && document.getElementById('sky').dataset.dragonIdsUnique === 'true'"
        ),
        35,
    )?;
    std::thread::sleep(Duration::from_secs(2));
    if value(follower, "dragonRole")? != "guest" || value(successor, "dragonRole")? != "host" {
        return Err("takeover did not stay converged".into());
    }
    successor.screenshot(&dir.join("handoff.png"))?;
    println!("PASS one successor after host loss, continued spawning with unique IDs");
    follower.eval("document.getElementById('pauseBtn').click();document.getElementById('leaveBtn').click();document.getElementById('mapPartyBtn').click();document.getElementById('soloBtn').click()")?;
    wait(
        follower,
        "document.documentElement.dataset.dragonLink === 'solo' && document.getElementById('sky').dataset.dragonFriends === '0'",
        10,
    )?;
    println!("PASS Fly solo disconnects the party cleanly");
    if let Some(error) = successor.trouble() {
        return Err(error);
    }
    if let Some(error) = follower.trouble() {
        return Err(error);
    }
    Ok(())
}
fn main() {
    let dir = std::env::var_os("DRAGON_LOGS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("dragon-p2p-{}", std::process::id())));
    let _ = std::fs::create_dir_all(&dir);
    match run(&dir) {
        Ok(()) => println!("Dragon P2P acceptance passed; evidence {}", dir.display()),
        Err(error) => {
            eprintln!(
                "Dragon P2P acceptance FAILED: {error}; evidence {}",
                dir.display()
            );
            std::process::exit(1);
        }
    }
}
