//! Production WASM audio in isolated, muted headless Chrome. No game or personal storage.
//! Build `dragon_audio_probe` for wasm32-unknown-unknown, run wasm-bindgen with
//! `--target web --no-typescript` into a fresh temporary directory, then run
//! `cargo run -p cooptest --bin dragon_audio -- <that directory>`.
//! Real WebAudio is exercised; speech service callbacks are deterministic synthetic
//! fixtures. Every assertion must return explicit success, because the shared CDP
//! helper deliberately returns null for JavaScript evaluation exceptions.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use chrome::{Chrome, Options, Result};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const HOOK: &str = r#"
window.audioTrace={nodes:[],params:[],disconnects:[],next:0};
const labels=new WeakMap();
for(const [method,kind] of [['createOscillator','osc'],['createGain','gain'],['createBufferSource','noise'],['createBiquadFilter','filter']]) {
 const original=BaseAudioContext.prototype[method];
 BaseAudioContext.prototype[method]=function(...args){const node=original.apply(this,args);node.testid=++audioTrace.next;node.testkind=kind;node.starts=[];node.stops=[];audioTrace.nodes.push(node);for(const field of ['frequency','gain'])if(node[field])labels.set(node[field],[node.testid,field]);return node;};
}
for(const method of ['setValueAtTime','exponentialRampToValueAtTime']) {const original=AudioParam.prototype[method];AudioParam.prototype[method]=function(...args){audioTrace.params.push([labels.get(this),method,...args]);return original.apply(this,args);};}
for(const method of ['start','stop']) {const original=AudioScheduledSourceNode.prototype[method];AudioScheduledSourceNode.prototype[method]=function(...args){this[method==='start'?'starts':'stops'].push(args);return original.apply(this,args);};}
const disconnect=AudioNode.prototype.disconnect;AudioNode.prototype.disconnect=function(...args){audioTrace.disconnects.push(this.testid);return disconnect.apply(this,args);};
window.resetTrace=()=>{audioTrace.nodes=[];audioTrace.params=[];audioTrace.disconnects=[];};
window.nativeSpeech={available:'speechSynthesis' in window, voices:window.speechSynthesis?.getVoices().length??0};
window.spoken=[];window.cancelCount=0;
const voices=[{name:'Samantha compact',lang:'en-US',default:true,localService:true},{name:'Ava Enhanced',lang:'en-US',default:false,localService:true},{name:'Siri Premium',lang:'fr-FR',default:true,localService:true}];
Object.defineProperty(window,'speechSynthesis',{configurable:true,value:{getVoices:()=>voices,cancel:()=>cancelCount++,speak:u=>spoken.push(u)}});
Object.defineProperty(window,'SpeechSynthesisUtterance',{configurable:true,value:class{constructor(text){this.text=text;}}});
window.check=(condition,message)=>{if(!condition)throw Error(message);};
window.closeTo=(a,b)=>Math.abs(a-b)<1e-5;
"#;
fn wait(b: &mut Chrome, expr: &str) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        if b.eval(expr)? == true {
            return Ok(());
        }
        if Instant::now() > end {
            return Err(format!("timed out: {expr}"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn test(b: &mut Chrome, name: &str, script: &str) -> Result<()> {
    let result = b.eval(&format!("(()=>{{try{{{script};return {{ok:true}};}}catch(error){{return {{ok:false,error:String(error.stack||error)}};}}}})()"))?;
    if result["ok"] != true {
        return Err(format!("{name}: {result}"));
    }
    println!("PASS {name}");
    Ok(())
}
fn run(root: &Path) -> Result<()> {
    let html = r#"<!doctype html><meta charset="utf-8"><title>Isolated audio probe</title><script type="module">import init,* as probe from './dragon_audio_probe.js';await init();window.probe=probe;window.ready=true;</script>"#;
    std::fs::write(root.join("index.html"), html).map_err(|e| e.to_string())?;
    let base = serve::start(root, 800, 600).map_err(|e| e.to_string())?;
    let profile = std::env::temp_dir().join(format!("dragon-audio-browser-{}", std::process::id()));
    let mut b = Chrome::start_without_clipboard(
        "audio",
        &profile,
        Options {
            fake_mic: false,
            slow: 1.,
        },
    )?;
    b.install_script(HOOK)?;
    b.go(&format!("{base}/index.html"))?;
    wait(&mut b, "window.ready===true")?;
    if test(
        &mut b,
        "negative assertion guard",
        "check(false,'expected sentinel')",
    )
    .is_ok()
    {
        return Err("browser assertion failures were swallowed".into());
    }
    println!("PASS harness rejects browser assertion failures");
    test(
        &mut b,
        "real WebAudio SFX pitch/envelope and source music bus",
        r#"
 probe.effect('pick');let counts=JSON.parse(probe.counts());check(counts.total===1&&counts.music===0,'SFX marker');
 let osc=audioTrace.nodes.find(n=>n.testkind==='osc');check(osc.type==='triangle','pick triangle');check(closeTo(osc.stops[0][0]-osc.starts[0][0],.13),'SFX stop time');
 check(audioTrace.params.some(p=>p[0][1]==='frequency'&&p[2]===660),'pick frequency');check(audioTrace.params.some(p=>p[1]==='exponentialRampToValueAtTime'&&closeTo(p[2],.12)&&closeTo(p[3]-osc.starts[0][0],.012)),'12ms SFX attack');
 check(audioTrace.nodes.some(n=>n.testkind==='gain'&&closeTo(n.gain.value,.45)),'source master');
 probe.music();counts=JSON.parse(probe.counts());check(counts.total===2&&counts.music===1,'music marker');check(audioTrace.nodes.some(n=>n.testkind==='gain'&&closeTo(n.gain.value,.45*.22)),'music bus');
 const musicNodes=audioTrace.nodes.filter(n=>n.testkind==='osc').slice(1);check(musicNodes.length===5,'calm chord bass lead');
 for(const node of musicNodes)check(audioTrace.params.some(p=>p[0][0]!==osc.testid&&p[1]==='exponentialRampToValueAtTime'&&closeTo(p[3]-node.starts[0][0],.02)),'20ms music attack');
 probe.stop(true);check(JSON.parse(probe.counts()).total===1,'music mute preserves SFX');check(osc.stops.length===1,'SFX not cut');check(musicNodes.every(n=>n.stops.length===2),'music nodes stopped');check(audioTrace.disconnects.length===15,'all music nodes disconnected');
 probe.stop(false);check(JSON.parse(probe.counts()).total===0,'master mute');check(osc.stops.length===2,'SFX canceled');check(audioTrace.disconnects.length===18,'all nodes disconnected');resetTrace();
 "#,
    )?;
    test(
        &mut b,
        "spark cadence, noise filters and bounded cleanup",
        r#"
 probe.effect('spark');probe.effect('spark');check(JSON.parse(probe.counts()).total===1,'alternating spark');check(audioTrace.nodes.filter(n=>n.testkind==='osc').length===1,'spark oscillator');probe.stop(false);resetTrace();
 probe.effect('freeze');const noise=audioTrace.nodes.find(n=>n.testkind==='noise');check(closeTo(noise.buffer.duration,.8),'shared 0.8s noise');const filter=audioTrace.nodes.find(n=>n.testkind==='filter');check(filter.type==='highpass'&&filter.frequency.value===3000,'freeze highpass');probe.reap(1);check(JSON.parse(probe.counts()).total===0,'expired group reaped');check(audioTrace.nodes.every(n=>audioTrace.disconnects.includes(n.testid)),'expired nodes disconnected');resetTrace();
 for(let i=0;i<40;i++)probe.effect('pick');check(JSON.parse(probe.counts()).total===32,'voice cap');check(audioTrace.nodes.filter(n=>n.testkind==='osc').length===32,'no excess nodes');probe.stop(false);check(audioTrace.nodes.every(n=>audioTrace.disconnects.includes(n.testid)),'cap cleanup');resetTrace();
 "#,
    )?;
    test(
        &mut b,
        "actual Rust narration callbacks with synthetic speech service",
        r#"
 check(probe.narrate('Hello world! "Stay close."'),'narration accepted');check(spoken.length===2,'sentence split');let first=spoken[0],last=spoken[1];check(first.voice.name==='Ava Enhanced','preferred English voice');check(closeTo(first.rate,1.04)&&closeTo(first.pitch,1.1),'excited narrator');check(closeTo(last.rate,.98)&&closeTo(last.pitch,.92),'character voice');
 first.onstart({});check(JSON.parse(probe.progress()).active.join(',')==='0,2','active range');first.onboundary({name:'word',charIndex:6});check(JSON.parse(probe.progress()).word===1,'word boundary');first.onend({});check(JSON.parse(probe.progress()).done===2,'completed range');last.onstart({});last.onend({});check(!JSON.parse(probe.progress()).speaking&&JSON.parse(probe.progress()).done===4,'complete');
 probe.cancel_narration();check(first.onstart===null&&last.onend===null,'callbacks detached on cancel');spoken=[];probe.narrate('A 🐉 bright sky.');spoken[0].onboundary({name:'word',charIndex:5});check(JSON.parse(probe.progress()).word===2,'UTF16 word offset');spoken[0].onerror({error:'network'});check(JSON.parse(probe.progress()).failed&&!JSON.parse(probe.progress()).speaking,'speech failure unlocks');probe.cancel_narration();
 "#,
    )?;
    test(
        &mut b,
        "narration failure clears highlights and timeout detaches callbacks",
        r#"
        spoken=[];probe.narrate("Two words.");spoken[0].onstart({});spoken[0].onboundary({name:"word",charIndex:4});spoken[0].onerror({error:"network"});
        let p=JSON.parse(probe.progress());check(!p.speaking&&p.failed&&p.done===2&&p.word===null&&p.active===null,"error completes and clears highlights");
        probe.cancel_narration();spoken=[];probe.narrate("Never finishes.");const old=spoken[0],now=Date.now;Date.now=()=>now()+60000;p=JSON.parse(probe.progress());Date.now=now;
        check(!p.speaking&&p.done===2&&p.word===null&&p.active===null,"timeout completion");check(old.onboundary===null&&old.onstart===null,"timeout detaches stale callbacks");probe.cancel_narration();
    "#,
    )?;
    println!(
        "Engine speech availability (not spoken/listened): {}",
        b.eval("nativeSpeech")?
    );
    if let Some(error) = b.trouble() {
        return Err(error);
    }
    drop(b);
    std::fs::remove_dir_all(profile).map_err(|e| e.to_string())?;
    Ok(())
}
fn main() {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: dragon_audio <isolated wasm-bindgen output directory>");
    if let Err(e) = run(&root) {
        eprintln!("FAIL {e}");
        std::process::exit(1);
    }
}
