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
