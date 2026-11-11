//! Matched lantern screenshots in disposable Chrome profiles. No production saves are touched.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[allow(dead_code)]
#[path = "../save_fixture.rs"]
mod save_fixture;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use base64::{Engine, engine::general_purpose::STANDARD};
use chrome::{Chrome, Options, Result};
use serde_json::{Value, json};
use std::{
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};

fn wait(b: &mut Chrome, cells: bool) -> Result<()> {
    let began = Instant::now();
    loop {
        if b.attr("data-crab").is_some()
            && (!cells || b.attr("data-cell-save").as_deref() == Some("loaded"))
        {
            return Ok(());
        }
        if let Some(e) = b.trouble() {
            return Err(e);
        }
        if began.elapsed().as_secs() > 90 {
            return Err("game fixture startup deadline".into());
        }
        sleep(Duration::from_millis(100));
    }
}

fn fixture(b: &mut Chrome, base: &str, cells: bool, x: f32, y: f32, dim: bool) -> Result<()> {
    b.go(&format!("{base}/__fixture_setup"))?;
    let player = crabsim::SaveData {
        version: crabsim::SAVE_VERSION,
        x,
        y,
        memory: None,
        progress: Some(crabsim::progress::Progress {
            lantern_dim: dim,
            ..Default::default()
        }),
    };
    if cells {
        let level = crabsim::Level::test_level();
        let mut terrain = crabsim::underwater::World::from_level_settled(&level);
        terrain.follow_player(&level, x, y);
        let bytes = crabsim::cell_codec::paged::prepare(&player, &terrain, 17)
            .and_then(|p| p.encode())
            .map_err(|e| format!("fixture: {e:?}"))?;
        let raw = save_fixture::spatial_object_from_frame(&bytes)?;
        b.eval(&format!(r#"new Promise((resolve,reject)=>{{const r=indexedDB.open('crabscape.cells',1);r.onupgradeneeded=()=>r.result.createObjectStore('world');r.onerror=()=>reject(String(r.error));r.onsuccess=()=>{{const db=r.result,tx=db.transaction('world','readwrite'),s=tx.objectStore('world'),raw={raw},decode=x=>Uint8Array.from(atob(x),c=>c.charCodeAt(0));s.clear();s.put(decode(raw.head),'active');s.put(decode(raw.epoch),'reset_epoch');for(const [k,v] of raw.records)s.put(decode(v),k);tx.oncomplete=()=>{{db.close();resolve(true)}};tx.onabort=()=>reject(String(tx.error));}}}})"#))?;
    } else {
        let save = serde_json::to_string(&player).map_err(|e| e.to_string())?;
        b.eval(&format!(
            "localStorage.setItem('crabscape.save',{})",
            serde_json::to_string(&save).map_err(|e| e.to_string())?
        ))?;
    }
    b.go(&format!(
        "{base}/?{}",
        if cells { "cells" } else { "boring" }
    ))?;
    wait(b, cells)
}

fn pixels(b: &mut Chrome, path: &Path) -> Result<Value> {
    let png = STANDARD.encode(std::fs::read(path).map_err(|e| e.to_string())?);
    b.eval(&format!(r#"new Promise(resolve=>{{const i=new Image;i.onload=()=>{{const c=document.createElement('canvas');c.width=i.width;c.height=i.height;const ctx=c.getContext('2d');ctx.drawImage(i,0,0);const p=ctx.getImageData(0,0,c.width,c.height).data,grid=[];for(let gy=0;gy<6;gy++)for(let gx=0;gx<8;gx++){{let n=0,sum=0;for(let y=Math.floor(gy*c.height/6);y<Math.floor((gy+1)*c.height/6);y++)for(let x=Math.floor(gx*c.width/8);x<Math.floor((gx+1)*c.width/8);x++){{const k=(y*c.width+x)*4;sum+=.2126*p[k]+.7152*p[k+1]+.0722*p[k+2];n++;}}grid.push(sum/n)}}resolve({{width:c.width,height:c.height,luma_grid_8x6:grid}})}};i.src='data:image/png;base64,{png}';}})"#))
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: lantern_check WEB_DIRECTORY OUTPUT_DIRECTORY LABEL".into());
    }
    let out = Path::new(&args[2]);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let base = serve::start(Path::new(&args[1]), 1280, 720).map_err(|e| e.to_string())?;
    let mut records = Vec::new();
    let phone = args[3].starts_with("phone");
    for cells in [false, true] {
        for (scene, x, y) in [("daylight", 220.0, 130.0), ("dark", 6000.0, -3380.0)] {
            for dim in [false, true] {
                if phone && (!cells || scene != "dark" || dim) {
                    continue;
                }
                if args[3] == "after-controls" && (scene != "dark" || dim) {
                    continue;
                }
                let name = format!(
                    "{}-{}-{scene}-{}",
                    args[3],
                    if cells { "cellular" } else { "legacy" },
                    if dim { "dim" } else { "full" }
                );
                let profile = std::env::temp_dir()
                    .join(format!("crab-lantern-{}-{name}", std::process::id()));
                let mut b = Chrome::start_without_clipboard(
                    &name,
                    &profile,
                    Options {
                        fake_mic: false,
                        slow: 1.0,
                    },
                )?;
                if phone {
                    b.responsive_viewport(390, 844)?;
                    b.touch_screen(true)?;
                } else {
                    b.protocol_viewport(1280, 720)?;
                }
                fixture(&mut b, &base, cells, x, y, dim)?;
                let settling = Instant::now();
                loop {
                    if b.attr("data-crab")
                        .is_some_and(|p| p.split_whitespace().nth(2) == Some("1"))
                    {
                        break;
                    }
                    if settling.elapsed().as_secs() > 90 {
                        return Err("fixture did not reach the ground before capture".into());
                    }
                    sleep(Duration::from_millis(100));
                }
                sleep(Duration::from_secs(2));
                let path = out.join(format!("{name}.png"));
                b.screenshot(&path)?;
                let record = json!({"name":name,"synthetic_spawn":[x,y],"position":b.attr("data-crab"),"dim_fixture":dim,"pixels":pixels(&mut b,&path)?});
                println!("{record}");
                records.push(record);
                // Show the actual rendered inventory state, not just the fixture value.
                b.hold_key("KeyB", 0.1)?;
                sleep(Duration::from_millis(300));
                b.screenshot(&out.join(format!("{name}-inventory.png")))?;
                // Coordinates are supplied only after inspecting this exact portrait layout.
                // CDP clicks test the visible pointer UI, not native iPhone touch dispatch.
                if phone && let Ok(point) = std::env::var("LANTERN_TAP") {
                    let (x, y) = point.split_once(',').ok_or("LANTERN_TAP requires x,y")?;
                    let x = x.parse::<f64>().map_err(|e| e.to_string())?;
                    let y = y.parse::<f64>().map_err(|e| e.to_string())?;
                    b.click(x, y)?;
                    sleep(Duration::from_secs(1));
                    b.screenshot(&out.join(format!("{name}-clicked-dim.png")))?;
                    b.click(x, y)?;
                    sleep(Duration::from_secs(1));
                    b.screenshot(&out.join(format!("{name}-clicked-full.png")))?;
                }
                if args[3].starts_with("after") && scene == "dark" && !dim {
                    // Holding H produces repeated key-down events but must toggle only once.
                    b.set_key("KeyH", true)?;
                    for _ in 0..4 {
                        sleep(Duration::from_millis(150));
                        b.set_key("KeyH", true)?;
                    }
                    b.set_key("KeyH", false)?;
                    sleep(Duration::from_secs(1));
                    b.screenshot(&out.join(format!("{name}-held-h-dim.png")))?;
                    if !cells {
                        let dim_saved = b.eval("JSON.parse(localStorage.getItem('crabscape.save')).progress.lantern_dim")?;
                        if dim_saved != json!(true) {
                            return Err(format!("repeated H did not save DIM: {dim_saved}"));
                        }
                    }
                    b.eval("window.lanternOldDocument=true")?;
                    b.reload()?;
                    let began = Instant::now();
                    while b.eval(
                        "window.lanternOldDocument !== true && document.readyState === 'complete'",
                    )? != json!(true)
                    {
                        if began.elapsed().as_secs() > 90 {
                            return Err("replacement document did not load".into());
                        }
                        sleep(Duration::from_millis(100));
                    }
                    wait(&mut b, cells)?;
                    sleep(Duration::from_secs(2));
                    b.hold_key("KeyB", 0.1)?;
                    sleep(Duration::from_millis(300));
                    b.screenshot(&out.join(format!("{name}-reloaded-dim.png")))?;
                    b.hold_key("KeyH", 0.1)?;
                    sleep(Duration::from_secs(1));
                    b.screenshot(&out.join(format!("{name}-h-full.png")))?;
                }
                if let Some(e) = b.trouble() {
                    return Err(e);
                }
                drop(b);
                std::fs::remove_dir_all(profile).map_err(|e| e.to_string())?;
            }
        }
    }
    std::fs::write(
        out.join(format!("{}-metrics.json", args[3])),
        serde_json::to_vec_pretty(&records).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
