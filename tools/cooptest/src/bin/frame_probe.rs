//! Full browser frame pacing during repeated ascent and descent, not a remap microbenchmark.
#[path = "frame_probe/canyon.rs"]
mod canyon;
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
use chrome::{Chrome, Options, Result};
use serde_json::{Value, json};
use std::{
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};

const CAPTURE: &str = r#"(()=>{
 const c=document.getElementById('crabscape');
 c.setAttribute('data-cell-work-profile','1');
 c.setAttribute('data-cell-performance-probe','1');
 const gl=c.getContext('webgl2'), ext=gl?.getExtension('WEBGL_debug_renderer_info');
 const s=window.frameProbe={frames:[],measures:[],longTasks:[],stop:false,
 renderer:ext?gl.getParameter(ext.UNMASKED_RENDERER_WEBGL):'unavailable',webgl:{}};
 const restore=[];
 if(gl)for(const name of ['createBuffer','deleteBuffer','bufferData','bufferSubData','createTexture','deleteTexture','texImage2D','texSubImage2D']){
  const original=gl[name];s.webgl[name]=0;
  gl[name]=function(...args){s.webgl[name]++;return original.apply(this,args)};
  restore.push(()=>{gl[name]=original});
 }
 for(const [proto,name,label] of [[IDBFactory.prototype,'open','open'],[IDBObjectStore.prototype,'get','get']]){
  const original=proto[name];
  proto[name]=function(...args){
   const at=performance.now(),request=original.apply(this,args);
   const record=()=>s.measures.push({at,ms:performance.now()-at,name:'cell-idb-'+label,key:label==='get'?String(args[0]):null});
   request.addEventListener('success',record,{once:true});request.addEventListener('error',record,{once:true});
   return request;
  };
  restore.push(()=>{proto[name]=original});
 }
 const observers=[];
 for(const type of ['measure','longtask']){
  if(!PerformanceObserver.supportedEntryTypes.includes(type))continue;
  const o=new PerformanceObserver(list=>{for(const e of list.getEntries()){
   if(type==='measure'&&!e.name.startsWith('cell-'))continue;
   s[type==='measure'?'measures':'longTasks'].push({at:e.startTime,ms:e.duration,name:e.name});
  }});o.observe({type,buffered:false});observers.push(o);
 }
 let previous=null;
 function frame(now){
  if(s.stop){observers.forEach(o=>o.disconnect());restore.forEach(f=>f());return;}
  if(previous!==null)s.frames.push({at:now,ms:now-previous,crab:c.getAttribute('data-crab'),
   tick:c.getAttribute('data-cell-performance')?.split(':')[0]??null,paging:c.getAttribute('data-cell-page-state'),revision:c.getAttribute('data-cell-region-revision')});
  previous=now;requestAnimationFrame(frame);
 }
 requestAnimationFrame(frame);return true;
})()"#;

fn summarize(capture: &Value, budget: Option<f64>) -> Result<Value> {
    let frames = capture["frames"].as_array().ok_or("no frame trace")?;
    if frames.len() < 30 {
        return Err("too few frames to measure the route".into());
    }
    let mut gaps = Vec::new();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut final_y = f64::INFINITY;
    let mut previous_tick = None;
    let mut tick_observations = 0;
    let mut unchanged_tick_ms: f64 = 0.0;
    let mut longest_unchanged_tick_ms: f64 = 0.0;
    let mut previous_position = None;
    let mut unchanged_ms: f64 = 0.0;
    let mut longest_unchanged_ms: f64 = 0.0;
    for f in frames {
        let ms = f["ms"]
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or("invalid frame interval")?;
        gaps.push(ms);
        let tick = f["tick"].as_str().and_then(|v| v.parse::<u64>().ok());
        if let Some(tick) = tick {
            tick_observations += 1;
            if previous_tick == Some(tick) {
                unchanged_tick_ms += ms;
                longest_unchanged_tick_ms = longest_unchanged_tick_ms.max(unchanged_tick_ms);
            } else {
                unchanged_tick_ms = 0.0;
            }
        } else {
            unchanged_tick_ms = 0.0;
        }
        previous_tick = tick;
        let pos: Vec<f64> = f["crab"]
            .as_str()
            .unwrap_or("")
            .split_whitespace()
            .take(2)
            .filter_map(|n| n.parse().ok())
            .collect();
        if pos.len() == 2 && pos.iter().all(|n| n.is_finite()) {
            let position = (pos[0], pos[1]);
            if previous_position == Some(position) {
                unchanged_ms += ms;
                longest_unchanged_ms = longest_unchanged_ms.max(unchanged_ms);
            } else {
                unchanged_ms = 0.0;
            }
            previous_position = Some(position);
            min_x = min_x.min(pos[0]);
            min_y = min_y.min(pos[1]);
            max_x = max_x.max(pos[0]);
            max_y = max_y.max(pos[1]);
            final_y = pos[1];
        } else {
            // Missing observations cannot establish continuous immobility.
            previous_position = None;
            unchanged_ms = 0.0;
        }
    }
    // A stationary scene must never count as a passing crossing benchmark.
    let complete = if capture["route"] == "canyon" {
        min_y < -3000.0 && max_y > -2000.0 && max_x - min_x > 300.0
    } else {
        max_x >= 1200.0 && max_y >= 2000.0 && final_y <= 500.0
    };
    if !complete {
        return Err(format!(
            "route incomplete: x={min_x}..{max_x}, y={min_y}..{max_y}, final y={final_y}"
        ));
    }
    gaps.sort_by(f64::total_cmp);
    let worst = *gaps.last().ok_or("no frame intervals")?;
    let result = json!({"frames":gaps.len(),"median_ms":gaps[gaps.len()/2],
        "p95_ms":gaps[gaps.len()*95/100],"max_ms":worst,"over_60_ms":gaps.iter().filter(|g| **g>60.0).count(),
        "tick_observations":tick_observations,"longest_unchanged_tick_ms":if tick_observations>0 {Some(longest_unchanged_tick_ms)} else {None},
        "longest_unchanged_position_ms":longest_unchanged_ms,"webgl_calls":capture["webgl"],
        "build":capture["build"],"cpu_throttle":capture["cpu_throttle"],"cpu_profile_enabled":capture["cpu_profile_enabled"],"route":capture["route"],"fixture":capture["fixture"],"min_x":min_x,"min_y":min_y,"max_x":max_x,"max_y":max_y,"final_y":final_y,"renderer":capture["renderer"]});
    println!("{result}");
    if budget.is_some_and(|limit| worst > limit) {
        return Err(format!(
            "maximum frame gap {worst:.2} ms exceeds calibrated budget {} ms",
            budget.unwrap()
        ));
    }
    Ok(result)
}

fn run() -> Result<()> {
    let mut url = "https://crabscape.com/".to_owned();
    let mut slow = 1.0;
    let mut budget = None;
    let mut cpu = false;
    let mut route = "opening".to_owned();
    let mut large = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => url = args.next().ok_or("--url needs a value")?,
            "--route" => route = args.next().ok_or("--route needs opening or canyon")?,
            "--large-save" => large = true,
            "--slow" => {
                slow = args
                    .next()
                    .ok_or("--slow needs a number")?
                    .parse::<f64>()
                    .map_err(|e| e.to_string())?
            }
            "--max-gap-ms" => {
                budget = Some(
                    args.next()
                        .ok_or("--max-gap-ms needs a number")?
                        .parse::<f64>()
                        .map_err(|e| e.to_string())?,
                )
            }
            "--cpu" => cpu = true,
            "--help" => {
                println!(
                    "frame_probe [--url URL] [--route opening|canyon] [--large-save] [--slow N] [--cpu] [--max-gap-ms N]\nOpening: 30-second ascent/descent. Canyon: 60-second rotation/paddle/dive from a synthetic save; --large-save adds explored memory/archive. Isolated temporary profile. Set CRABSCAPE_PROFILE_GPU=1 for hardware rendering. No timing gate unless a calibrated budget is supplied."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    if !slow.is_finite() || slow < 1.0 || budget.is_some_and(|n| !n.is_finite() || n <= 0.0) {
        return Err("slow must be finite and >=1; budget must be finite and >0".into());
    }
    if !matches!(route.as_str(), "opening" | "canyon") || (large && route != "canyon") {
        return Err("route must be opening or canyon; --large-save requires --route canyon".into());
    }
    let dir = std::env::temp_dir().join(format!("crab-frame-probe-{}", std::process::id()));
    std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
    let mut b = Chrome::start_without_clipboard(
        "probe",
        &dir.join("profile"),
        Options {
            fake_mic: false,
            slow,
            ..Options::default()
        },
    )?;
    b.go(&url)?;
    let start = Instant::now();
    while b.attr("data-crab").is_none() {
        if start.elapsed().as_secs() > 60 {
            return Err("game did not start within 60 seconds".into());
        }
        sleep(Duration::from_millis(200));
    }
    let fixture = if route == "canyon" {
        canyon::prepare(&mut b, &url, large)?
    } else {
        Value::Null
    };
    let build = b.eval("fetch(new URL('pkg/commit.txt',location.href)).then(r=>r.ok?r.text():'unavailable').catch(()=> 'unavailable')")?;
    b.type_line("hulk")?;
    if route == "canyon" {
        b.type_line("level swimming 99")?;
        b.type_line("feast")?;
    }
    if cpu {
        b.start_cpu_profile()?;
    }
    b.eval(CAPTURE)?;
    if route == "canyon" {
        canyon::drive(&mut b)?;
    } else {
        b.set_key("KeyD", true)?;
        let began = Instant::now();
        while began.elapsed().as_secs() < 30 {
            b.hold_key(
                if began.elapsed().as_secs() < 15 {
                    "KeyW"
                } else {
                    "ArrowDown"
                },
                0.25,
            )?;
            sleep(Duration::from_millis(150));
        }
        b.set_key("KeyD", false)?;
    }
    let mut capture = b.eval("(()=>{window.frameProbe.stop=true;document.getElementById('crabscape').removeAttribute('data-cell-work-profile');document.getElementById('crabscape').removeAttribute('data-cell-performance-probe');return window.frameProbe})()")?;
    capture["route"] = json!(route);
    capture["fixture"] = fixture;
    capture["build"] = build;
    capture["cpu_throttle"] = json!(slow);
    capture["cpu_profile_enabled"] = json!(cpu);
    write(&dir.join("frames.json"), &capture)?;
    if cpu {
        write(&dir.join("cpu.json"), &b.stop_cpu_profile()?)?;
    }
    println!("Trace: {}", dir.display());
    if let Some(error) = b.trouble() {
        return Err(error);
    }
    let summary = summarize(&capture, budget)?;
    write(&dir.join("summary.json"), &summary)
}
fn write(path: &Path, value: &Value) -> Result<()> {
    std::fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("frame probe: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn trace(gap: f64) -> Value {
        json!({"frames":(0..60).map(|i|json!({"ms":if i==25 {gap}else{16.7},"crab":if i<30 {"1400 2500 0"}else{"1800 0 1"}})).collect::<Vec<_>>()})
    }
    #[test]
    fn catches_a_single_long_frame_even_with_a_healthy_median() {
        assert!(summarize(&trace(16.7), Some(60.0)).is_ok());
        assert!(
            summarize(&trace(233.0), Some(60.0))
                .unwrap_err()
                .contains("maximum frame gap")
        );
    }
    #[test]
    fn canyon_requires_deep_water_and_actual_travel() {
        let mut v = trace(16.7);
        v["route"] = json!("canyon");
        assert!(
            summarize(&v, None).is_err(),
            "opening travel is not canyon coverage"
        );
        for (i, f) in v["frames"].as_array_mut().unwrap().iter_mut().enumerate() {
            f["crab"] = json!(if i < 30 {
                "6250 -4200 0"
            } else {
                "6900 -1800 0"
            });
        }
        assert!(summarize(&v, Some(60.0)).is_ok());
        for f in v["frames"].as_array_mut().unwrap() {
            f["crab"] = json!("6250 -4200 0");
        }
        assert!(
            summarize(&v, None).is_err(),
            "stationary deep water is not travel"
        );
    }
    #[test]
    fn reports_immobility_separately_from_render_pacing() {
        let result = summarize(&trace(16.7), None).unwrap();
        assert!((result["longest_unchanged_position_ms"].as_f64().unwrap() - 484.3).abs() < 0.01);
        assert_eq!(result["max_ms"], 16.7);
        let mut v = trace(16.7);
        for i in [14, 44] {
            v["frames"][i]["crab"] = Value::Null;
        }
        let result = summarize(&v, None).unwrap();
        assert!((result["longest_unchanged_position_ms"].as_f64().unwrap() - 233.8).abs() < 0.01);
    }
    #[test]
    fn tick_stalls_are_independent_of_position_and_missing_samples_break_them() {
        let mut v = trace(16.7);
        assert!(summarize(&v, None).unwrap()["longest_unchanged_tick_ms"].is_null());
        for (i, frame) in v["frames"].as_array_mut().unwrap().iter_mut().enumerate() {
            frame["tick"] = json!(i.to_string());
        }
        let result = summarize(&v, None).unwrap();
        assert_eq!(result["longest_unchanged_tick_ms"], 0.0);
        for frame in v["frames"].as_array_mut().unwrap() {
            frame["tick"] = json!("10");
        }
        v["frames"][30]["tick"] = Value::Null;
        let result = summarize(&v, None).unwrap();
        assert!((result["longest_unchanged_tick_ms"].as_f64().unwrap() - 484.3).abs() < 0.01);
        assert_eq!(result["tick_observations"], 59);
    }
    #[test]
    fn stationary_scene_cannot_pass() {
        let mut v = trace(16.7);
        for f in v["frames"].as_array_mut().unwrap() {
            f["crab"] = json!("269 258 0");
        }
        assert!(
            summarize(&v, None)
                .unwrap_err()
                .contains("route incomplete")
        );
    }
}
