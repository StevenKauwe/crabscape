//! Peak memory on an iPhone-sized screen (390 x 844 at 3x), so a phone's browser never kills the game.
//!
//! An iPhone kills a browser tab that grows past roughly 1 to 1.5 GB in all: the page flashes white and reloads, and
//! after a few tries Chrome says "Can't open this page". A desktop browser and the iOS Simulator don't enforce that
//! limit, so the game can be fine everywhere we test and still die on a real phone. This plays the game at phone size
//! and density in Chrome and records, every quarter second:
//!
//! - the game's WebAssembly memory (`memory.buffer.byteLength`: it only ever grows, so its peak is what the phone
//!   pays for the whole visit);
//! - every WebGL texture, renderbuffer and buffer the game allocates, in bytes (tallied by wrapping the WebGL calls
//!   before the game's scripts run), plus the canvas's own drawing buffers.
//!
//! The route: stand at home, swim out to the first snapping shrimp (x 340) and back three times, then swim on, up
//! and right through the open water, across several terrain window shifts (the active terrain is 640 x 360 cells;
//! the route must end over 1,500 units from home). With `--large-save` it first installs a large explored save (the same
//! synthetic world the frame probe uses), so a long-played phone save is covered too.
//!
//! It fails when either peak passes its budget (`--max-wasm-mib`, `--max-gpu-mib`; defaults below). The budgets sit
//! well under a phone's limit, which also has to hold the browser itself, its JavaScript and the decoded pages.
//!
//! Usage: `memory_budget WEB_DIRECTORY [--query quality=high] [--large-save] [--out DIR] [--max-wasm-mib N] [--max-gpu-mib N] [--max-buffers-per-frame N]`
//! or `memory_budget --url URL ...` for a deployed build.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[allow(dead_code)]
#[path = "../save_fixture.rs"]
mod save_fixture;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use chrome::{Chrome, Options, Result};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    thread::sleep,
    time::{Duration, Instant},
};

/// The game's WebAssembly memory may peak at this many MiB (it never shrinks).
const MAX_WASM_MIB: f64 = 300.0;
/// All WebGL textures, renderbuffers, buffers and the canvas's drawing buffers together, in MiB.
const MAX_GPU_MIB: f64 = 160.0;
/// New WebGL buffers a frame while the crab idles at home. Each is a GPU allocation the iPhone's Safari
/// charges to the page: at about 330 a frame (11,000 a second: a new material for every sprite whose
/// colour changed), the page passed 2 GB while WebGL itself held 100 MiB (see `src/sprite_steps.rs`).
const MAX_BUFFERS_PER_FRAME: f64 = 30.0;

/// Installed before any of the page's scripts: keeps each WebAssembly memory and tallies WebGL allocations.
const INSTRUMENT: &str = r#"(()=>{
 const M=window.__memories=[];
 const keep=r=>{const i=r&&(r.instance||r);for(const v of Object.values((i&&i.exports)||{}))if(v instanceof WebAssembly.Memory&&!M.includes(v))M.push(v);return r;};
 for(const n of ['instantiate','instantiateStreaming']){const f=WebAssembly[n];WebAssembly[n]=function(...a){return f.apply(this,a).then(keep)};}
 const G=window.__growth=[];let last=0;const t0=performance.now();
 setInterval(()=>{const n=M.reduce((a,m)=>a+m.buffer.byteLength,0);if(n!==last){last=n;
  const r=performance.getEntriesByType('resource');G.push([Math.round(performance.now()-t0),Math.round(n/1048576),r.length,(r[r.length-1]||{}).name]);}},5);
 const P=WebGL2RenderingContext.prototype;
 const BPP={0x8058:4,0x8C43:4,0x822B:2,0x8229:1,0x881A:8,0x8814:16,0x88F0:4,0x8CAC:4,0x8CAD:8,0x8C3A:4,0x8059:4,
  0x822D:2,0x822F:4,0x822E:4,0x8230:8,0x81A6:4,0x81A5:2,0x8D7C:4,0x8D8E:4,0x8D70:16,0x8D62:2,0x8051:3,0x8C41:3,0x8D48:1,0x1908:4,0x1907:3};
 const s=window.__gl={tex:0,rb:0,buf:0,peak:0,peakAt:null,unknown:{},objects:new Map(),uploads:0,uploadBytes:0};
 const state=new WeakMap();
 const st=gl=>{let x=state.get(gl);if(!x){x={unit:0x84C0,tex:{},buf:{},rb:null};state.set(gl,x);}return x;};
 const bpp=f=>{if(BPP[f])return BPP[f];s.unknown[f]=(s.unknown[f]||0)+1;return 4;};
 const set=(kind,obj,bytes,info)=>{if(!obj)return;const old=s.objects.get(obj);if(old)s[old.kind]-=old.bytes;
  s.objects.set(obj,{kind,bytes,...info});s[kind]+=bytes;const total=s.tex+s.rb+s.buf;if(total>s.peak){s.peak=total;}};
 const drop=obj=>{const old=s.objects.get(obj);if(old){s[old.kind]-=old.bytes;s.objects.delete(obj);}};
 const wrap=(name,f)=>{const o=P[name];if(!o)return;P[name]=function(...a){f(this,a);return o.apply(this,a)};};
 wrap('activeTexture',(gl,a)=>{st(gl).unit=a[0]});
 wrap('bindTexture',(gl,a)=>{const x=st(gl);(x.tex[x.unit]??={})[a[0]]=a[1]});
 wrap('bindBuffer',(gl,a)=>{st(gl).buf[a[0]]=a[1]});
 wrap('bindRenderbuffer',(gl,a)=>{st(gl).rb=a[1]});
 const boundTex=(gl,t)=>{const x=st(gl);return (x.tex[x.unit]||{})[t===0x8515||(t>0x8515&&t<=0x851A)?0x8513:t]};
 wrap('texStorage2D',(gl,[t,levels,f,w,h])=>{let b=0;for(let l=0;l<levels;l++)b+=Math.max(1,w>>l)*Math.max(1,h>>l);
  set('tex',boundTex(gl,t),b*bpp(f)*(t===0x8513?6:1),{w,h,d:t===0x8513?6:1,levels,format:f})});
 wrap('texStorage3D',(gl,[t,levels,f,w,h,d])=>{let b=0;for(let l=0;l<levels;l++)b+=Math.max(1,w>>l)*Math.max(1,h>>l)*(t===0x806F?Math.max(1,d>>l):d);
  set('tex',boundTex(gl,t),b*bpp(f),{w,h,d,levels,format:f})});
 wrap('texImage2D',(gl,a)=>{const [t,level,f]=a;let w,h;if(a.length>=8){w=a[3];h=a[4];}else{w=a[5]?.width;h=a[5]?.height;}
  if(level!==0||!w||!h)return;set('tex',boundTex(gl,t),w*h*bpp(f),{w,h,d:1,levels:1,format:f,image:true})});
 for(const n of ['texSubImage2D','texSubImage3D']) wrap(n,(gl,a)=>{s.uploads++;const v=a.find(x=>ArrayBuffer.isView(x));if(v)s.uploadBytes+=v.byteLength;});
 wrap('deleteTexture',(gl,a)=>drop(a[0]));
 wrap('renderbufferStorage',(gl,[t,f,w,h])=>set('rb',st(gl).rb,w*h*bpp(f),{w,h,samples:1,format:f}));
 wrap('renderbufferStorageMultisample',(gl,[t,n,f,w,h])=>set('rb',st(gl).rb,w*h*bpp(f)*Math.max(1,n),{w,h,samples:n,format:f}));
 wrap('deleteRenderbuffer',(gl,a)=>drop(a[0]));
 wrap('bufferData',(gl,a)=>{const n=typeof a[1]==='number'?a[1]:(a[4]!==undefined?a[4]*(a[1].BYTES_PER_ELEMENT||1):(a[1]?.byteLength||0));
  set('buf',st(gl).buf[a[0]],n,{})});
 wrap('deleteBuffer',(gl,a)=>drop(a[0]));
 s.created=0;s.frames=0;wrap('createBuffer',()=>{s.created++});
 (function f(){s.frames++;requestAnimationFrame(f)})();
})()"#;

/// One sample of everything we count.
const SAMPLE: &str = r#"(()=>{const c=document.getElementById('crabscape'),s=window.__gl;
 const gl=c&&c.getContext('webgl2'),attrs=gl&&gl.getContextAttributes();
 const drawing=c?c.width*c.height*(4*2+(attrs&&attrs.depth?4:0))*(attrs&&attrs.antialias?4:1):0;
 return {at:performance.now(),wasm:(window.__memories||[]).map(m=>m.buffer.byteLength),
  tex:s.tex,rb:s.rb,buf:s.buf,created:s.created,frames:s.frames,drawing,canvas:c?[c.width,c.height]:null,
  js:performance.memory?performance.memory.usedJSHeapSize:null,crab:c&&c.getAttribute('data-crab'),
  page:c&&c.getAttribute('data-cell-page-state')}})()"#;

/// The largest allocations right now, for the report.
const LARGEST: &str = r#"(()=>{const s=window.__gl;return [...s.objects.values()].sort((a,b)=>b.bytes-a.bytes).slice(0,40)
 .map(o=>({kind:o.kind,mib:+(o.bytes/1048576).toFixed(2),w:o.w,h:o.h,d:o.d,levels:o.levels,samples:o.samples,format:o.format&&('0x'+o.format.toString(16))}))
 .concat([{unknown_formats:s.unknown,texture_uploads:s.uploads,upload_mib:+(s.uploadBytes/1048576).toFixed(1),
  textures:[...s.objects.values()].filter(o=>o.kind==='tex').length}])})()"#;

const MIB: f64 = 1024.0 * 1024.0;

/// Chrome on an iPhone 15 Pro Max (every browser on iOS is WebKit underneath).
const IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) \
    CriOS/131.0.6778.73 Mobile/15E148 Safari/604.1";

struct Probe {
    b: Chrome,
    profile: PathBuf,
    samples: Vec<Value>,
    phase: String,
}

impl Probe {
    fn sample(&mut self) -> Result<Value> {
        let mut v = self.b.eval(SAMPLE)?;
        v["phase"] = json!(self.phase);
        let (renderer, gpu) = process_rss(&self.profile);
        v["renderer_rss"] = json!(renderer);
        v["gpu_rss"] = json!(gpu);
        self.samples.push(v.clone());
        if let Some(error) = self.b.trouble() {
            return Err(format!(
                "{}: the page reported trouble: {error}",
                self.phase
            ));
        }
        Ok(v)
    }
    fn x(v: &Value) -> Option<f64> {
        v["crab"].as_str()?.split(' ').next()?.parse().ok()
    }
    fn idle(&mut self, phase: &str, secs: f64) -> Result<()> {
        self.phase = phase.into();
        let began = Instant::now();
        while began.elapsed().as_secs_f64() < secs {
            self.sample()?;
            sleep(Duration::from_millis(250));
        }
        Ok(())
    }
    /// Swim towards `x` (hopping over the ground's bumps) until there, or `secs` run out.
    fn swim_to(&mut self, phase: &str, x: f64, secs: f64) -> Result<f64> {
        self.phase = phase.into();
        let mut at = self.sample().ok().and_then(|v| Self::x(&v)).unwrap_or(0.0);
        let key = if x > at { "KeyD" } else { "KeyA" };
        self.b.set_key(key, true)?;
        let began = Instant::now();
        while began.elapsed().as_secs_f64() < secs
            && (x - at).abs() > 40.0
            && (x > at) == (key == "KeyD")
        {
            let v = self.sample()?;
            let now = Self::x(&v).unwrap_or(at);
            // Stuck on a bump: hop over it.
            if (now - at).abs() < 2.0 {
                self.b.hold_key("KeyW", 0.15)?;
            }
            at = now;
            sleep(Duration::from_millis(150));
        }
        self.b.set_key(key, false)?;
        Ok(at)
    }
}

/// Resident memory of this browser's page processes (the page, its workers) and its GPU process, in bytes, from
/// `ps` (macOS and Linux). Zero where `ps` isn't available.
fn process_rss(profile: &Path) -> (f64, f64) {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-axww", "-o", "pid=,ppid=,rss=,command="])
        .output()
    else {
        return (0.0, 0.0);
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<(u32, u32, f64, &str)> = text
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let ppid = it.next()?.parse().ok()?;
            let rss: f64 = it.next()?.parse().ok()?;
            let at = l.find(" /").or_else(|| l.rfind(char::is_numeric))?;
            Some((pid, ppid, rss * 1024.0, &l[at..]))
        })
        .collect();
    let mark = format!("--user-data-dir={}", profile.display());
    let mut family: Vec<u32> = rows
        .iter()
        .filter(|r| r.3.contains(&mark) && !r.3.contains("--type="))
        .map(|r| r.0)
        .collect();
    let mut i = 0;
    while i < family.len() {
        let parent = family[i];
        family.extend(rows.iter().filter(|r| r.1 == parent).map(|r| r.0));
        i += 1;
    }
    let sum = |kind: &str| -> f64 {
        rows.iter()
            .filter(|r| family.contains(&r.0) && r.3.contains(kind))
            .map(|r| r.2)
            .sum()
    };
    (sum("--type=renderer"), sum("--type=gpu-process"))
}

/// Install a large explored save with the crab at home.
fn install_large_save(b: &mut Chrome, url: &str) -> Result<()> {
    let level = crabsim::Level::test_level();
    let saved = crabsim::cell_codec::paged::decode(&save_fixture::large_fixture()?)
        .map_err(|e| format!("large fixture: {e:?}"))?;
    let (mut terrain, mut player) = (saved.terrain, saved.player);
    player.x = crabsim::SPAWN.x;
    player.y = crabsim::SPAWN.y;
    if !terrain.follow_player(&level, player.x, player.y) {
        return Err("the large save did not stream to home".into());
    }
    let bytes = crabsim::cell_codec::paged::prepare(&player, &terrain, 17)
        .and_then(|p| p.encode())
        .map_err(|e| format!("fixture: {e:?}"))?;
    println!("large save: {:.1} MiB", bytes.len() as f64 / MIB);
    let fixture = save_fixture::spatial_object_from_frame(&bytes)?;
    let database = b.eval(r#"(()=>{const p=location.pathname.split('/').filter(Boolean);let s=null;
      for(let i=0;i<p.length;i++){if(/^pr-\d+$/.test(p[i])){s=p[i];break;}if(p[i]==='branches'&&p[i+1]){s='branches-'+p[i+1];break;}}
      return s?'crabscape@'+s.replace(/[^a-zA-Z0-9_-]/g,'-').toLowerCase()+'@.cells':'crabscape.cells';})()"#)?;
    let asset = b
        .eval("new URL('assets/crab/crab-atlas-oil.png',location.href).href")?
        .as_str()
        .ok_or("missing asset route")?
        .to_owned();
    // Leave the game before installing the save so autosave cannot race it.
    b.go(&asset)?;
    b.eval(&format!(r#"new Promise((resolve,reject)=>{{
      const r=indexedDB.open({database},1);r.onerror=()=>reject(String(r.error));
      r.onupgradeneeded=()=>r.result.createObjectStore('world');
      r.onsuccess=()=>{{const db=r.result,tx=db.transaction('world','readwrite'),s=tx.objectStore('world'),raw={fixture};
        const decode=x=>Uint8Array.from(atob(x),c=>c.charCodeAt(0));s.clear();
        s.put(decode(raw.head),'active');s.put(decode(raw.epoch),'reset_epoch');
        for(const [key,value] of raw.records)s.put(decode(value),key);
        tx.oncomplete=()=>{{db.close();resolve(true)}};tx.onabort=()=>{{db.close();reject(String(tx.error))}};
      }};}})"#))?;
    b.go(url)?;
    let began = Instant::now();
    while b.attr("data-cell-save").as_deref() != Some("loaded") || b.attr("data-crab").is_none() {
        if let Some(error) = b.trouble() {
            return Err(error);
        }
        if began.elapsed().as_secs() > 90 {
            return Err("the large save did not load".into());
        }
        sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn peak(samples: &[Value], f: impl Fn(&Value) -> f64) -> (f64, String) {
    samples
        .iter()
        .map(|s| (f(s), s["phase"].as_str().unwrap_or("").to_string()))
        .fold((0.0, String::new()), |a, b| if b.0 > a.0 { b } else { a })
}

/// New WebGL buffers a frame over the samples of one phase.
fn buffers_per_frame(samples: &[Value], phase: &str) -> f64 {
    let mine: Vec<&Value> = samples.iter().filter(|s| s["phase"] == phase).collect();
    let (Some(a), Some(b)) = (mine.get(mine.len() / 4), mine.last()) else {
        return 0.0;
    };
    let frames = b["frames"].as_f64().unwrap_or(0.0) - a["frames"].as_f64().unwrap_or(0.0);
    let made = b["created"].as_f64().unwrap_or(0.0) - a["created"].as_f64().unwrap_or(0.0);
    if frames > 0.0 { made / frames } else { 0.0 }
}

fn wasm_bytes(s: &Value) -> f64 {
    s["wasm"]
        .as_array()
        .map_or(0.0, |m| m.iter().filter_map(Value::as_f64).sum())
}

fn gpu_bytes(s: &Value) -> f64 {
    ["tex", "rb", "buf", "drawing"]
        .iter()
        .filter_map(|k| s[k].as_f64())
        .sum()
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut web, mut url, mut large, mut out) = (None, None, false, None);
    let mut query = String::new();
    let (mut max_wasm, mut max_gpu) = (MAX_WASM_MIB, MAX_GPU_MIB);
    let mut max_churn = MAX_BUFFERS_PER_FRAME;
    let number = |v: Option<String>, what: &str| -> Result<f64> {
        v.ok_or(format!("{what} needs a number"))?
            .parse::<f64>()
            .map_err(|e| e.to_string())
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => url = args.next(),
            "--large-save" => large = true,
            "--query" => query = args.next().ok_or("--query needs e.g. quality=high")?,
            "--out" => out = args.next().map(PathBuf::from),
            "--max-wasm-mib" => max_wasm = number(args.next(), "--max-wasm-mib")?,
            "--max-gpu-mib" => max_gpu = number(args.next(), "--max-gpu-mib")?,
            "--max-buffers-per-frame" => {
                max_churn = number(args.next(), "--max-buffers-per-frame")?
            }
            "--help" => {
                println!(
                    "memory_budget WEB_DIRECTORY | --url URL [--query Q] [--large-save] [--out DIR] [--max-wasm-mib N] [--max-gpu-mib N] [--max-buffers-per-frame N]"
                );
                return Ok(());
            }
            other if web.is_none() && !other.starts_with("--") => web = Some(PathBuf::from(other)),
            other => return Err(format!("unknown option {other}")),
        }
    }
    let url = match (url, web) {
        (Some(url), _) => url,
        (None, Some(web)) => {
            format!(
                "{}/",
                serve::start(&web, 390, 844).map_err(|e| e.to_string())?
            )
        }
        (None, None) => return Err("give a web directory or --url".into()),
    };
    let url = if query.is_empty() {
        url
    } else {
        format!("{url}?{query}")
    };
    let dir = std::env::temp_dir().join(format!("crab-memory-budget-{}", std::process::id()));
    let mut b = Chrome::start_without_clipboard("phone", &dir.join("profile"), Options::default())?;
    b.device_metrics(390, 844, 3.0)?;
    // An iPhone's browser, so the game picks the tier it picks on a real phone (`src/quality.rs`).
    b.user_agent(IPHONE)?;
    b.touch_screen(true)?;
    b.install_script(INSTRUMENT)?;
    b.go(&url)?;
    let began = Instant::now();
    while b.attr("data-crab").is_none() {
        if let Some(error) = b.trouble() {
            return Err(error);
        }
        if began.elapsed().as_secs() > 90 {
            return Err("the game did not start within 90 seconds".into());
        }
        sleep(Duration::from_millis(200));
    }
    if large {
        install_large_save(&mut b, &url)?;
    }
    let mut p = Probe {
        b,
        profile: dir.join("profile"),
        samples: Vec::new(),
        phase: "start".into(),
    };
    p.idle("home", 12.0)?;
    p.b.type_line("hulk")?;
    p.b.type_line("level swimming 99")?;
    for trip in 1..=3 {
        p.swim_to(&format!("to the shrimp {trip}"), 360.0, 25.0)?;
        p.idle(&format!("at the shrimp {trip}"), 2.0)?;
        p.swim_to(&format!("back home {trip}"), crabsim::SPAWN.x as f64, 25.0)?;
    }
    p.swim_to("across window shifts", 2200.0, 60.0)?;
    p.idle("after the window shifts", 4.0)?;
    let home = crabsim::SPAWN;
    let reached = p
        .samples
        .iter()
        .filter_map(|s| {
            let mut it = s["crab"]
                .as_str()?
                .split(' ')
                .map(|v| v.parse::<f32>().ok());
            Some((it.next()?? - home.x).hypot(it.next()?? - home.y))
        })
        .fold(0.0_f32, f32::max) as f64;
    let largest = p.b.eval(LARGEST)?;
    let (wasm, wasm_at) = peak(&p.samples, wasm_bytes);
    let (gpu, gpu_at) = peak(&p.samples, gpu_bytes);
    let (tex, _) = peak(&p.samples, |s| s["tex"].as_f64().unwrap_or(0.0));
    let (rb, _) = peak(&p.samples, |s| s["rb"].as_f64().unwrap_or(0.0));
    let (js, _) = peak(&p.samples, |s| s["js"].as_f64().unwrap_or(0.0));
    let (renderer, renderer_at) = peak(&p.samples, |s| s["renderer_rss"].as_f64().unwrap_or(0.0));
    let (gpu_rss, _) = peak(&p.samples, |s| s["gpu_rss"].as_f64().unwrap_or(0.0));
    let first = p.samples.first().cloned().unwrap_or(Value::Null);
    let churn = buffers_per_frame(&p.samples, "home");
    let summary = json!({
        "large_save": large,
        "canvas": first["canvas"],
        "farthest_from_home": reached,
        "wasm_start_mib": wasm_bytes(&first) / MIB,
        "wasm_peak_mib": wasm / MIB, "wasm_peak_phase": wasm_at,
        "gpu_peak_mib": gpu / MIB, "gpu_peak_phase": gpu_at,
        "texture_peak_mib": tex / MIB, "renderbuffer_peak_mib": rb / MIB,
        "drawing_buffer_mib": first["drawing"].as_f64().unwrap_or(0.0) / MIB,
        "js_heap_peak_mib": js / MIB,
        "page_process_peak_mib": renderer / MIB, "page_process_peak_phase": renderer_at,
        "gpu_process_peak_mib": gpu_rss / MIB,
        "buffers_per_frame_at_home": churn,
        "budget": {"wasm_mib": max_wasm, "gpu_mib": max_gpu, "buffers_per_frame": max_churn},
        "largest_allocations": largest,
        "wasm_growth": p.b.eval("window.__growth")?,
        "quality": p.b.attr("data-quality"),
        "big_allocations": p.b.eval("window.__big||null")?,
    });
    let out = out.unwrap_or(dir);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let write = |name: &str, v: &Value| -> Result<()> {
        std::fs::write(
            out.join(name),
            serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    };
    write("samples.json", &json!(p.samples))?;
    write("summary.json", &summary)?;
    println!(
        "canvas {} | WebAssembly memory {:.0} MiB at start, peak {:.0} MiB ({wasm_at}) | WebGL {:.0} MiB peak \
         ({gpu_at}): textures {:.0}, renderbuffers {:.0}, drawing buffer {:.0} | farthest {reached:.0} from home | {churn:.1} new WebGL buffers a frame at home | page process {:.0} MiB, GPU process {:.0} MiB | {}",
        first["canvas"],
        wasm_bytes(&first) / MIB,
        wasm / MIB,
        gpu / MIB,
        tex / MIB,
        rb / MIB,
        first["drawing"].as_f64().unwrap_or(0.0) / MIB,
        renderer / MIB,
        gpu_rss / MIB,
        out.join("summary.json").display()
    );
    if reached < 1500.0 {
        return Err(format!(
            "the crab only got {reached:.0} from home: the route must cross terrain window shifts"
        ));
    }
    let mut over = Vec::new();
    if wasm / MIB > max_wasm {
        over.push(format!(
            "WebAssembly memory peaked at {:.0} MiB (budget {max_wasm:.0})",
            wasm / MIB
        ));
    }
    if gpu / MIB > max_gpu {
        over.push(format!(
            "WebGL memory peaked at {:.0} MiB (budget {max_gpu:.0})",
            gpu / MIB
        ));
    }
    if churn > max_churn {
        over.push(format!(
            "{churn:.1} new WebGL buffers a frame at home (budget {max_churn:.0}): a sprite or material \
             changes every frame (see src/sprite_steps.rs)"
        ));
    }
    if !over.is_empty() {
        return Err(format!(
            "over the phone's memory budget: {}; the largest allocations are in {}",
            over.join("; "),
            out.join("summary.json").display()
        ));
    }
    println!("PASS within the phone's memory budget");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("memory budget: {error}");
        std::process::exit(1);
    }
}
