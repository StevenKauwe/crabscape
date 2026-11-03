//! Production IndexedDB proofs for the bounded Canyon owner namespace.
use super::*;
use crabsim::canyon_drift::run::{CanyonRun, GrantPhase, SlotPhase};
use serde_json::Value;

fn database(player: &mut Player) -> Result<String> {
    player.b.eval(r#"(()=>{const p=location.pathname.split('/').filter(Boolean);let s=null;
      for(let i=0;i<p.length;i++){if(/^pr-\d+$/.test(p[i])){s=p[i];break;}if(p[i]==='branches'&&p[i+1]){s='branches-'+p[i+1];break;}}
      return s?'crabscape@'+s.replace(/[^a-zA-Z0-9_-]/g,'-').toLowerCase()+'@.cells':'crabscape.cells';})()"#)?
        .as_str().map(str::to_owned).ok_or("missing test database".into())
}
fn install(player: &mut Player, name: &str, fixture: &Value) -> Result<()> {
    player.b.eval(&format!(r#"new Promise((resolve,reject)=>{{
      const opening=indexedDB.open({name:?},1);opening.onerror=()=>reject(String(opening.error));
      opening.onsuccess=()=>{{const db=opening.result,tx=db.transaction('world','readwrite'),s=tx.objectStore('world'),raw={fixture};
        const decode=v=>Uint8Array.from(atob(v),c=>c.charCodeAt(0));s.clear();s.put(decode(raw.head),'active');s.put(decode(raw.epoch),'reset_epoch');
        for(const [key,bytes] of raw.records)s.put(decode(bytes),key);
        tx.onabort=()=>{{db.close();reject(String(tx.error));}};tx.oncomplete=()=>{{db.close();resolve(true);}};
      }};}})"#))?;
    Ok(())
}
fn capture(player: &mut Player) -> Result<Value> {
    player.b.eval(save_fixture::CAPTURE_SCRIPT)
}
fn claim_run(value: &Value) -> Result<CanyonRun> {
    let records =
        save_fixture::canyon_from_object(value)?.ok_or("Canyon owner bundle is missing")?;
    let run = CanyonRun::from_records(&records).ok_or("Canyon owner bundle is incomplete")?;
    if run.world_id != 71 || records.len() != 5 {
        return Err("Canyon lost its namespace or one of its five owner pages".into());
    }
    for (index, slot) in run.slots.iter().enumerate() {
        let SlotPhase::Grant(grant) = slot.phase else {
            return Err("Canyon reused a slot before its guest receipt was durably retired".into());
        };
        if slot.generation != 7 + index as u32
            || grant.id.generation != slot.generation
            || grant.id.world != 71
            || grant.identity != 9000 + index as u64
            || !matches!(grant.phase, GrantPhase::Requested | GrantPhase::Offered)
        {
            return Err("Canyon advanced an absent recipient's generation or grant".into());
        }
    }
    Ok(run)
}
fn saved_tick(value: &Value) -> Result<u64> {
    crabsim::cell_codec::paged::decode(&save_fixture::frame_from_object(value)?)
        .map(|saved| saved.sim_tick)
        .map_err(|e| format!("invalid Canyon player/terrain: {e:?}"))
}
fn wait_save(player: &mut Player, after: u64) -> Result<Value> {
    let began = Instant::now();
    loop {
        let value = capture(player)?;
        if !value.is_null() && saved_tick(&value)? > after {
            claim_run(&value)?;
            return Ok(value);
        }
        player.healthy()?;
        if began.elapsed().as_secs() > 60 {
            return Err(player.failed("Canyon wrote no complete newer save in 60s".into()));
        }
        sleep(0.5);
    }
}
pub(super) fn run(player: &mut Player, site: &Site) -> Result<Found> {
    let mut found = Found::new();
    player.open(&site.url("cells"))?;
    let name = database(player)?;
    let asset = player
        .b
        .eval("new URL('assets/crab/crab-atlas.png',location.href).href")?
        .as_str()
        .ok_or("missing asset route")?
        .to_owned();
    player.b.go(&asset)?;
    let fixture = save_fixture::canyon_fixture()?;
    claim_run(&fixture)?;
    install(player, &name, &fixture)?;
    player.open(&site.url("cells"))?;
    player.until(20.0, "five-page Canyon save restores", |p| {
        p.b.attr("data-cell-save").as_deref() == Some("loaded")
    })?;

    player.b.eval(r#"(()=>{window.__canyonOriginalPut=IDBObjectStore.prototype.put;window.__canyonAborts=0;
      IDBObjectStore.prototype.put=function(value,key){if(this.name==='world'&&key==='active'){window.__canyonAborts++;throw new DOMException('Canyon final-head failure','QuotaExceededError');}
        return window.__canyonOriginalPut.call(this,value,key);};return true;})()"#)?;
    let before = capture(player)?;
    claim_run(&before)?;
    let before_tick = saved_tick(&before)?;
    player.until(60.0, "Canyon head publication aborts", |p| {
        p.b.eval("window.__canyonAborts>0").ok() == Some(Value::Bool(true))
    })?;
    if capture(player)? != before {
        return Err(
            player.failed("failed Canyon commit changed player or coordinate records".into())
        );
    }
    player
        .b
        .eval("(()=>{IDBObjectStore.prototype.put=window.__canyonOriginalPut;return true;})()")?;
    let committed = wait_save(player, before_tick)?;
    let committed_tick = saved_tick(&committed)?;
    found.push("final-head failure rolls back player, terrain and all five Canyon pages; retry commits complete generation-keyed claims".into());

    player.reload()?;
    player.until(20.0, "Canyon survives a real reload", |p| {
        p.b.attr("data-cell-save").as_deref() == Some("loaded")
    })?;
    let reloaded = wait_save(player, committed_tick)?;
    claim_run(&reloaded)?;
    player
        .b
        .eval("document.getElementById('crabscape').removeAttribute('data-replay')")?;
    player.b.hold_key("F9", 0.15)?;
    player.until(20.0, "Canyon diagnostic snapshot", |p| {
        p.b.attr("data-replay").is_some()
    })?;
    let text = player
        .b
        .attr("data-replay")
        .ok_or("missing Canyon diagnostic replay")?;
    let replay = crabsim::Replay::parse(&text).map_err(|e| format!("Canyon diagnostic: {e:?}"))?;
    let mut restored = crabsim::Sim::with_one_crab(replay.start);
    if !replay.prepare_start(&mut restored) || restored.canyon_records().is_empty() {
        return Err(
            "browser Canyon runtime was not restored into its native diagnostic snapshot".into(),
        );
    }
    let runtime = CanyonRun::from_records(&restored.canyon_records())
        .ok_or("runtime Canyon owners incomplete")?;
    for (index, slot) in runtime.slots.iter().enumerate() {
        if slot.generation != 7 + index as u32 || !matches!(slot.phase, SlotPhase::Grant(_)) {
            return Err("browser reload reset a pending Canyon claim in RAM".into());
        }
    }
    found.push(
        "reload and native diagnostic restore retain both pending generations without slot reuse"
            .into(),
    );

    player.b.go(&asset)?;
    let healthy = capture(player)?;
    claim_run(&healthy)?;
    player.b.eval(&format!(r#"new Promise((resolve,reject)=>{{const o=indexedDB.open({name:?},1);o.onerror=()=>reject(String(o.error));o.onsuccess=()=>{{
      const db=o.result,tx=db.transaction('world','readwrite');tx.objectStore('world').delete(['00000000000000000000','canyon_index',0,0,'00000000000000000000']);
      tx.onabort=()=>{{db.close();reject(String(tx.error));}};tx.oncomplete=()=>{{db.close();resolve(true);}};}};}})"#))?;
    let missing = capture(player)?;
    if save_fixture::canyon_from_object(&missing).is_ok() {
        return Err("fixture failed to remove its published Canyon index".into());
    }
    player.open(&site.url("cells"))?;
    player.until(20.0, "missing published Canyon index is rejected", |p| {
        p.b.attr("data-cell-save").as_deref() == Some("load-error")
    })?;
    if capture(player)? != missing {
        return Err("failed Canyon restore overwrote the damaged save".into());
    }
    found.push(
        "missing published index fails closed and preserves the stored player and Canyon rows"
            .into(),
    );

    player.b.go(&asset)?;
    install(player, &name, &healthy)?;
    // Retired envelopes start a fresh epoch; no Canyon records are migrated.
    player.b.eval(&format!(r#"new Promise((resolve,reject)=>{{const o=indexedDB.open({name:?},1);o.onerror=()=>reject(String(o.error));o.onsuccess=()=>{{
      const db=o.result,tx=db.transaction('world','readwrite'),s=tx.objectStore('world'),r=s.get('active');r.onsuccess=()=>{{const raw=r.result;raw.set(new TextEncoder().encode('CRABSP03'),0);s.put(raw,'active');}};
      tx.onabort=()=>{{db.close();reject(String(tx.error));}};tx.oncomplete=()=>{{db.close();resolve(true);}};}};}})"#))?;
    player.open(&site.url("cells"))?;
    player.until(20.0, "retired Canyon envelope starts fresh", |p| {
        p.b.attr("data-cell-save").as_deref() == Some("fresh")
    })?;
    let began = Instant::now();
    loop {
        let value = capture(player)?;
        if let Some(records) = save_fixture::canyon_from_object(&value)? {
            let run = CanyonRun::from_records(&records).ok_or("fresh Canyon bundle incomplete")?;
            if run
                .slots
                .iter()
                .any(|slot| matches!(slot.phase, SlotPhase::Grant(_)))
            {
                return Err("retired Canyon claims leaked into the fresh world".into());
            }
            break;
        }
        if began.elapsed().as_secs() > 60 {
            return Err("fresh Canyon world did not commit in 60s".into());
        }
        sleep(0.5);
    }
    found.push(
        "format retirement advances reset epoch and a fresh save contains no old pending claims"
            .into(),
    );
    Ok(found)
}

/// Start with a single physically caught generation, then use ordinary player input.
pub(super) fn play(host: &mut Player, guest: &mut Player, site: &Site) -> Result<Found> {
    host.open(&site.url("cells"))?;
    let name = database(host)?;
    let asset = host
        .b
        .eval("new URL('assets/crab/crab-atlas.png',location.href).href")?
        .as_str()
        .ok_or("missing asset route")?
        .to_owned();
    host.b.go(&asset)?;
    install(host, &name, &save_fixture::canyon_play_fixture()?)?;
    host.open(&site.url("cells"))?;
    host.until(30.0, "Canyon play fixture restores", |p| {
        p.b.attr("data-cell-save").as_deref() == Some("loaded")
    })?;
    guest.open(&site.url("cells"))?;
    let code = host.invite()?;
    let invite = host.copied_invite(&code, "cells")?;
    guest.open(&invite)?;
    guest.b.click(600.0, 400.0)?;
    guest.b.set_key("KeyA", true)?;
    for p in [&mut *host, &mut *guest] {
        p.until(90.0, "shared Canyon region", |p| {
            p.b.attr("data-cell-region-state").as_deref() == Some("running")
                && p.b.attr("data-cell-regions").as_deref() == Some("shared")
        })?;
    }
    guest.b.set_key("KeyA", false)?;
    eprintln!(
        "Canyon approach host {:?}, guest {:?}",
        local_position(host),
        local_position(guest)
    );
    guest.b.click(600.0, 400.0)?;
    for _ in 0..24 {
        let (x, _) = local_position(guest)?;
        if (6385.0..=6445.0).contains(&x) {
            break;
        }
        guest
            .b
            .hold_key(if x > 6445.0 { "KeyA" } else { "KeyD" }, 0.12)?;
        sleep(0.1);
    }
    eprintln!("Canyon guest after approach {:?}", local_position(guest));
    let approach = Instant::now();
    loop {
        let (x, y) = local_position(guest)?;
        if (x - 6416.0).hypot(y + 24.0 + 3165.0) < 65.0 {
            guest.b.hold_key("KeyG", 0.15)?;
            break;
        }
        if approach.elapsed().as_secs() > 90 {
            return Err(format!("guest could not reach Canyon catcher: ({x}, {y})"));
        }
        if (x - 6416.0).abs() > 15.0 {
            guest
                .b
                .hold_key(if x > 6416.0 { "KeyA" } else { "KeyD" }, 0.12)?;
        }
        sleep(0.15);
    }
    guest
        .b
        .screenshot(&site.dir.join("canyon-after-claim.png"))?;
    let began = Instant::now();
    loop {
        let host_value = capture(host)?;
        let guest_value = capture(guest)?;
        let run = save_fixture::canyon_from_object(&host_value)
            .ok()
            .flatten()
            .and_then(|records| CanyonRun::from_records(&records));
        let progress = save_fixture::frame_from_object(&guest_value)
            .ok()
            .and_then(|bytes| crabsim::cell_codec::paged::decode(&bytes).ok())
            .and_then(|save| save.player.progress);
        if let (Some(run), Some(progress)) = (run, progress)
            && (run.slots[0].generation > 1
                || (run.slots[0].generation == 1
                    && matches!(run.slots[0].phase, SlotPhase::Vacant)))
            && progress.drift_receipts.is_empty()
            && progress
                .clinging
                .iter()
                .filter(|item| **item == crabsim::Carry::Valve)
                .count()
                == 1
        {
            break;
        }
        if began.elapsed().as_secs() > 120 {
            return Err(format!(
                "Canyon guest claim did not durably retire; host {:?}; guest {:?}; progress {:?}",
                save_fixture::canyon_from_object(&host_value),
                local_position(guest),
                save_fixture::frame_from_object(&guest_value)
                    .ok()
                    .and_then(|bytes| crabsim::cell_codec::paged::decode(&bytes).ok())
                    .and_then(|save| save.player.progress)
            ));
        }
        host.healthy()?;
        guest.healthy()?;
        sleep(0.5);
    }
    guest.reload()?;
    guest.b.click(600.0, 400.0)?;
    guest.b.set_key("KeyA", true)?;
    guest.until(90.0, "guest rejoins after Canyon receipt retirement", |p| {
        p.b.attr("data-cell-session").as_deref() == Some("guest")
            && p.b.attr("data-cell-region-state").as_deref() == Some("running")
    })?;
    guest.b.set_key("KeyA", false)?;
    let saved = capture(guest)?;
    let progress = save_fixture::frame_from_object(&saved)
        .ok()
        .and_then(|bytes| crabsim::cell_codec::paged::decode(&bytes).ok())
        .and_then(|save| save.player.progress)
        .ok_or("guest progress missing after reload")?;
    if !progress.drift_receipts.is_empty()
        || progress
            .clinging
            .iter()
            .filter(|item| **item == crabsim::Carry::Valve)
            .count()
            != 1
    {
        return Err("guest reload did not retain exactly one valve and clear receipt".into());
    }
    let settle = Instant::now();
    loop {
        let (x, y) = local_position(guest)?;
        let grounded = guest
            .b
            .attr("data-crab")
            .is_some_and(|value| value.split_whitespace().last() == Some("1"));
        if (6400.0..=6440.0).contains(&x) && (-3200.0..=-3160.0).contains(&y) && grounded {
            break;
        }
        if settle.elapsed().as_secs() > 90 {
            return Err(format!(
                "guest could not settle at Canyon after reload: ({x}, {y})"
            ));
        }
        if !(6408.0..=6432.0).contains(&x) {
            guest
                .b
                .hold_key(if x > 6432.0 { "KeyA" } else { "KeyD" }, 0.12)?;
        }
        sleep(0.15);
    }
    // Send the original owner home using the visible button, leaving the guest
    // to own the distant Canyon region before it departs.
    host.b.click(1140.0, 70.0)?;
    for p in [&mut *host, &mut *guest] {
        p.until(90.0, "Canyon authority separates from the owner", |p| {
            p.b.attr("data-cell-regions").as_deref() == Some("separate")
                && p.b.attr("data-cell-region-state").as_deref() == Some("running")
        })?;
    }
    pause_region(guest, true)?;
    let region = regional_scene(guest)?;
    let canyon = region
        .canyon
        .as_ref()
        .ok_or("guest lost Canyon authority")?;
    if !canyon.owned_slots[0]
        || canyon.slots[0].generation < 1
        || (canyon.slots[0].generation == 1 && !matches!(canyon.slots[0].phase, SlotPhase::Vacant))
    {
        return Err("handoff lost or resurrected the claimed Canyon generation".into());
    }
    guest.b.go("about:blank")?;
    host.until(30.0, "owner resumes alone", |p| {
        p.b.attr("data-cell-session").as_deref() == Some("solo")
    })?;
    host.reload()?;
    host.until(30.0, "Canyon owner reloads", |p| {
        p.b.attr("data-cell-save").as_deref() == Some("loaded")
    })?;
    let saved = capture(host)?;
    let run = save_fixture::canyon_from_object(&saved)?
        .and_then(|records| CanyonRun::from_records(&records))
        .ok_or("owner Canyon records missing after reload")?;
    if run.slots[0].generation < 1
        || (run.slots[0].generation == 1 && !matches!(run.slots[0].phase, SlotPhase::Vacant))
    {
        return Err("owner reload resurrected the claimed generation".into());
    }
    Ok(vec!["guest physically claims one caught generation; player receipt and owner retirement survive separate reloads and distant regional authority handoff".into()])
}
