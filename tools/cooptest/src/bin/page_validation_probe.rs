//! Fault-inject final page validation in disposable synthetic worlds.
#[allow(dead_code)]
#[path = "frame_probe/canyon.rs"]
mod canyon;
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
use chrome::{Chrome, Options, Result};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};
fn main() -> Result<()> {
    let url = std::env::args()
        .nth(1)
        .ok_or("supply isolated candidate URL")?;
    let slow = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|pair| pair[0] == "--slow")
        .map(|pair| pair[1].parse::<f64>().map_err(|e| e.to_string()))
        .transpose()?
        .unwrap_or(1.0);
    let dir =
        std::env::temp_dir().join(format!("crab-page-validation-probe-{}", std::process::id()));
    let mut browser = Chrome::start_without_clipboard(
        "validation",
        &dir,
        Options {
            fake_mic: false,
            slow,
            ..Options::default()
        },
    )?;
    browser.go(&url)?;
    let start = Instant::now();
    while browser.attr("data-crab").is_none() {
        if start.elapsed().as_secs() > 60 {
            return Err("game startup deadline".into());
        }
        sleep(Duration::from_millis(100));
    }
    if std::env::args().any(|arg| arg == "--reuse-only") {
        canyon::prepare(&mut browser, &url, true)?;
        let expect_fresh = std::env::args().any(|arg| arg == "--expect-fresh-connections");
        let script = r#"(async()=>{
 const c=document.getElementById('crabscape');c.setAttribute('data-cell-work-profile','1');c.setAttribute('data-cell-performance-probe','1');
 const started=performance.now(),names=['cell-page-open','cell-page-read-total','cell-page-batch-read','cell-page-decode','cell-page-final-validation'];
 const original=IDBObjectStore.prototype.get,transactions=new Map(),entries=[],totals=[];
 let finished=null,observedApplied=0,failed=0;
 IDBObjectStore.prototype.get=function(key){
   const request=original.call(this,key),tx=this.transaction;
   if(tx.mode==='readonly'&&Array.isArray(key)&&['terrain','memory'].includes(key[1])){
     if(!transactions.has(tx)){
       const record={start:performance.now(),db:tx.db,keys:[],completed:false,aborted:false};transactions.set(tx,record);
       tx.addEventListener('complete',()=>record.completed=true,{once:true});tx.addEventListener('abort',()=>record.aborted=true,{once:true});
     }transactions.get(tx).keys.push(key);
   }return request;
 };
 const observer=new PerformanceObserver(list=>{for(const e of list.getEntries()){
   if(finished!==null)continue;
   if(names.includes(e.name)&&e.startTime>=started){entries.push({name:e.name,start:e.startTime,end:e.startTime+e.duration,duration:e.duration});if(e.name==='cell-page-read-total')totals.push({start:e.startTime,end:e.startTime+e.duration});}
   if(e.name==='cell-page-reply-failed')failed++;
   if(e.name==='cell-page-reply-applied'){observedApplied++;if(totals.length===8)finished=e.startTime;}
 }});observer.observe({type:'measure',buffered:false});
 try{
   while(finished===null&&performance.now()-started<30000)await new Promise(r=>setTimeout(r,25));
   if(finished===null||totals.length!==8||failed)throw Error(JSON.stringify({finished,completeReads:totals.length,failed}));
   const measures={};for(const name of names)measures[name]=entries.filter(e=>e.name===name&&totals.some(t=>e.start>=t.start-0.01&&e.end<=t.end+0.01)).map(e=>e.duration);
   const selected=[...transactions.values()].filter(r=>totals.some(t=>r.start>=t.start&&r.start<=t.end)),databases=new Map();
   for(const r of selected)databases.set(r.db,(databases.get(r.db)||0)+1);
   const result={batches:selected.length,completed:selected.filter(r=>r.completed).length,aborted:selected.filter(r=>r.aborted).length,applied:8,observedApplied,failed,elapsedMs:finished-totals[0].start,measures,keys:selected.flatMap(r=>r.keys),distinctPageConnections:databases.size,batchesPerConnection:[...databases.values()]};
   if(names.some(name=>measures[name].length!==8)||selected.length!==8||result.completed!==8||result.aborted||databases.size!==(EXPECT_FRESH?8:1))throw Error(JSON.stringify(result));
   result.build=(await (await fetch(new URL('pkg/commit.txt',location.href),{cache:'no-store'})).text()).trim();return result;
 }finally{IDBObjectStore.prototype.get=original;observer.disconnect();c.removeAttribute('data-cell-work-profile');c.removeAttribute('data-cell-performance-probe')}
})().catch(error=>({probeError:String(error)}))"#.replace("EXPECT_FRESH", if expect_fresh { "true" } else { "false" });
        let result = checked_timing_result(eval_checked(&mut browser, &script)?)?;
        println!("{result}");
        return browser.trouble().map_or(Ok(()), Err);
    }
    for mode in ["epoch", "revision", "head-epoch", "missing", "damaged"] {
        canyon::prepare(&mut browser, &url, true)?;
        let mode_json = serde_json::to_string(mode).map_err(|e| e.to_string())?;
        let script = r#"(async()=>{
          const mode=MODE,c=document.getElementById('crabscape');
          c.setAttribute('data-cell-work-profile','1');c.setAttribute('data-cell-performance-probe','1');
          const original=IDBObjectStore.prototype.get,transactions=new WeakMap(),events=[];
          let injected=null,failed=false,appliedBeforeFailure=false,recovered=false;
          const observer=new PerformanceObserver(list=>{for(const e of list.getEntries()){
            if(injected===null||e.startTime<injected)continue;
            if(e.name==='cell-page-reply-failed')failed=true;
            if(e.name==='cell-page-reply-applied'){if(!failed)appliedBeforeFailure=true;else recovered=true}
            if(e.name==='cell-page-reply-failed'||e.name==='cell-page-reply-applied')events.push(e.name);
          }});observer.observe({type:'measure',buffered:false});
          IDBObjectStore.prototype.get=function(key){
            const tx=this.transaction;
            if(!transactions.has(tx))transactions.set(tx,key);
            const request=original.call(this,key);
            const final=tx.mode==='readonly'&&transactions.get(tx)==='reset_epoch';
            if(final&&injected===null&&((mode==='epoch'&&key==='reset_epoch')||(mode!=='epoch'&&key==='active'))){
              request.addEventListener('success',()=>{
                let fake;
                if(mode==='epoch'){fake=new Uint8Array(8);new DataView(fake.buffer).setBigUint64(0,999n,true)}
                else if(mode==='missing')fake=undefined;
                else if(mode==='damaged')fake=new Uint8Array([0]);
                else {
                  fake=Uint8Array.from(request.result);
                  const epochAt=9+(fake[8]===1?32:0);
                  if(fake[epochAt]!==0||fake[epochAt+1]>=126)throw Error("unexpected fixture head encoding");
                  if(mode==="head-epoch")fake[epochAt]=1;else fake[epochAt+1]++;
                }
                Object.defineProperty(request,'result',{value:fake});injected=performance.now();
              },{once:true});
            }
            return request;
          };
          try {
            const start=performance.now();
            while(!recovered&&performance.now()-start<15000)await new Promise(r=>setTimeout(r,50));
            if(injected===null||!failed||!recovered||appliedBeforeFailure)throw Error(JSON.stringify({mode,injected,failed,recovered,appliedBeforeFailure,events}));
            return {mode,rejected:true,recovered:true,appliedBeforeFailure,events};
          } finally {IDBObjectStore.prototype.get=original;observer.disconnect();c.removeAttribute('data-cell-work-profile');c.removeAttribute('data-cell-performance-probe')}
        })()"#.replace("MODE", &mode_json);
        println!("{}", eval_checked(&mut browser, &script)?);
    }
    for mode in ["closed-handle", "versionchange", "timeout"] {
        canyon::prepare(&mut browser, &url, true)?;
        let mode_json = serde_json::to_string(mode).map_err(|e| e.to_string())?;
        let script = r#"(async()=>{
          const mode=MODE,c=document.getElementById('crabscape');
          c.setAttribute('data-cell-work-profile','1');c.setAttribute('data-cell-performance-probe','1');
          const original=IDBObjectStore.prototype.get,originalClose=IDBDatabase.prototype.close;
          let lastDb=null,faultDb=null,blockedTx=null,injected=null,failed=false,replacementSeen=false,recovered=false,closed=0,upgradeAborted=false;
          const events=[];
          IDBDatabase.prototype.close=function(){if(this===faultDb)closed++;return originalClose.call(this)};
          IDBObjectStore.prototype.get=function(key){
            const request=original.call(this,key);
            if(Array.isArray(key)&&['terrain','memory'].includes(key[1])){
              const db=this.transaction.db;lastDb=db;
              if(faultDb&&db!==faultDb)replacementSeen=true;
              if(mode==='timeout'&&injected===null){faultDb=db;blockedTx=this.transaction;injected=performance.now()}
              if(this.transaction===blockedTx)request.addEventListener('success',e=>e.stopImmediatePropagation(),{once:true});
            }
            return request;
          };
          const observer=new PerformanceObserver(list=>{for(const e of list.getEntries()){
            if(e.name==='cell-page-reply-applied'&&injected===null&&lastDb&&mode!=='timeout'){
              faultDb=lastDb;injected=performance.now();
              if(mode==='closed-handle')faultDb.close();else {const upgrade=indexedDB.open(faultDb.name,faultDb.version+1);upgrade.onupgradeneeded=()=>upgrade.transaction.abort();upgrade.onerror=e=>{e.preventDefault();upgradeAborted=true};}
            }
            if(injected===null||e.startTime<injected)continue;
            if(e.name==='cell-page-reply-failed')failed=true;
            if(e.name==='cell-page-reply-applied'&&replacementSeen&&(failed||mode==='versionchange'))recovered=true;
            if(e.name==='cell-page-reply-failed'||e.name==='cell-page-reply-applied')events.push(e.name);
          }});observer.observe({type:'measure',buffered:false});
          try{
            const start=performance.now();while(!recovered&&performance.now()-start<25000)await new Promise(r=>setTimeout(r,50));
            if(injected===null||!recovered||!replacementSeen||closed===0||(mode==='versionchange'&&!upgradeAborted)||(mode!=='versionchange'&&!failed))throw Error(JSON.stringify({mode,injected,failed,replacementSeen,recovered,closed,upgradeAborted,events}));
            return {mode,failed,replacementSeen,recovered,closed,upgradeAborted,events};
          }finally{IDBObjectStore.prototype.get=original;IDBDatabase.prototype.close=originalClose;observer.disconnect();c.removeAttribute('data-cell-work-profile');c.removeAttribute('data-cell-performance-probe')}
        })()"#.replace("MODE", &mode_json);
        println!("{}", eval_checked(&mut browser, &script)?);
    }
    if let Some(error) = browser.trouble() {
        return Err(error);
    }
    Ok(())
}

// Chrome::eval does not reject Runtime.evaluate exceptionDetails. Catch locally
// and validate the envelope so a JavaScript assertion cannot become a false pass.
fn eval_checked(browser: &mut Chrome, script: &str) -> Result<serde_json::Value> {
    let value = browser.eval(&format!(
        "Promise.resolve({script}).catch(error=>({{probeError:String(error)}}))"
    ))?;
    if value.as_object().is_none_or(|object| object.is_empty()) || value.get("probeError").is_some()
    {
        return Err(format!("page probe failed: {value}"));
    }
    Ok(value)
}

fn checked_timing_result(value: serde_json::Value) -> Result<serde_json::Value> {
    if value["applied"].as_u64() != Some(8)
        || value["completed"].as_u64() != Some(8)
        || value["failed"].as_u64() != Some(0)
        || value["aborted"].as_u64() != Some(0)
        || value.get("probeError").is_some()
        || !value["build"].is_string()
        || !value["keys"].is_array()
        || !value["elapsedMs"].is_number()
        || [
            "cell-page-open",
            "cell-page-read-total",
            "cell-page-batch-read",
            "cell-page-decode",
            "cell-page-final-validation",
        ]
        .iter()
        .any(|name| {
            value["measures"][name]
                .as_array()
                .is_none_or(|v| v.len() != 8 || v.iter().any(|x| !x.is_number()))
        })
    {
        return Err(format!("invalid production timing result: {value}"));
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_or_exception_results_cannot_pass() {
        for value in [
            serde_json::json!({}),
            serde_json::Value::Null,
            serde_json::json!({"probeError":"assertion failed"}),
            serde_json::json!({"applied":8}),
        ] {
            assert!(checked_timing_result(value).is_err());
        }
    }
}
