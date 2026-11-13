//! Disclosed synthetic canyon starts, installed only in the probe's disposable profile.
#[allow(dead_code)]
#[path = "../../save_fixture.rs"]
mod save_fixture;
use crate::{Chrome, Result};
use serde_json::{Value, json};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};

pub fn prepare(b: &mut Chrome, url: &str, large: bool) -> Result<Value> {
    let level = crabsim::Level::test_level();
    let (mut terrain, mut player) = if large {
        let saved = crabsim::cell_codec::paged::decode(&save_fixture::large_fixture()?)
            .map_err(|e| format!("large fixture: {e:?}"))?;
        (saved.terrain, saved.player)
    } else {
        (
            crabsim::underwater::World::from_level_settled(&level),
            crabsim::SaveData {
                version: crabsim::SAVE_VERSION,
                x: 6250.0,
                y: -5000.0,
                memory: None,
                progress: None,
            },
        )
    };
    player.x = 6250.0;
    player.y = -5000.0;
    if !terrain.follow_player(&level, player.x, player.y) {
        return Err("canyon fixture did not stream".into());
    }
    let bytes = crabsim::cell_codec::paged::prepare(&player, &terrain, 17)
        .and_then(|p| p.encode())
        .map_err(|e| format!("fixture: {e:?}"))?;
    let fixture = save_fixture::spatial_object_from_frame(&bytes)?;
    let description = json!({"synthetic":true,"large_world":large,"spawn":[6250,-5000],
        "save_version":crabsim::SAVE_VERSION,"paged_bytes":bytes.len(),
        "spatial_records":fixture["records"].as_array().map(Vec::len)});
    let database = b.eval(r#"(()=>{const p=location.pathname.split('/').filter(Boolean);let s=null;
      for(let i=0;i<p.length;i++){if(/^pr-\d+$/.test(p[i])){s=p[i];break;}if(p[i]==='branches'&&p[i+1]){s='branches-'+p[i+1];break;}}
      return s?'crabscape@'+s.replace(/[^a-zA-Z0-9_-]/g,'-').toLowerCase()+'@.cells':'crabscape.cells';})()"#)?;
    let asset = b
        .eval("new URL('assets/crab/crab-atlas-oil.png',location.href).href")?
        .as_str()
        .ok_or("missing asset route")?
        .to_owned();
    // Leave the game before installing the fixture so autosave cannot race it.
    b.go(&asset)?;
    b.eval(&format!(r#"new Promise((resolve,reject)=>{{
      const r=indexedDB.open({database},1);r.onerror=()=>reject(String(r.error));
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
        if began.elapsed().as_secs() > 60 {
            return Err("canyon fixture did not load; check game/fixture versions".into());
        }
        sleep(Duration::from_millis(100));
    }
    Ok(description)
}

pub fn drive(b: &mut Chrome) -> Result<()> {
    let began = Instant::now();
    let mut phase = 99;
    while began.elapsed().as_secs() < 60 {
        let next = began.elapsed().as_secs() / 20;
        if next != phase {
            release(b)?;
            if next == 1 {
                b.set_key("ArrowDown", true)?;
            } else {
                b.set_key("Space", true)?;
                b.set_key(if next == 0 { "KeyD" } else { "KeyA" }, true)?;
            }
            phase = next;
        }
        if phase == 1 {
            sleep(Duration::from_millis(200));
        } else {
            b.hold_key("KeyW", 0.25)?;
            sleep(Duration::from_millis(150));
        }
    }
    release(b)
}
fn release(b: &mut Chrome) -> Result<()> {
    for key in ["Space", "KeyD", "KeyA", "ArrowDown"] {
        b.set_key(key, false)?;
    }
    Ok(())
}
