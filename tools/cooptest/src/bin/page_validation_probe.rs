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
    let dir =
        std::env::temp_dir().join(format!("crab-page-validation-probe-{}", std::process::id()));
    let mut browser = Chrome::start_without_clipboard(
        "validation",
        &dir,
        Options {
            fake_mic: false,
            slow: 1.0,
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
        let result = browser.eval(r#"(async()=>{
          const c=document.getElementById('crabscape');
          c.setAttribute('data-cell-work-profile','1');c.setAttribute('data-cell-performance-probe','1');
          const original=IDBObjectStore.prototype.get,transactions=new WeakSet(),databases=new Map();
          let batches=0,completed=0,aborted=0,applied=0,failed=0;
          IDBObjectStore.prototype.get=function(key){
            const request=original.call(this,key),tx=this.transaction;
            if(tx.mode==='readonly'&&Array.isArray(key)&&['terrain','memory'].includes(key[1])&&!transactions.has(tx)){
              transactions.add(tx);batches++;databases.set(tx.db,(databases.get(tx.db)||0)+1);
              tx.addEventListener('complete',()=>completed++,{once:true});
              tx.addEventListener('abort',()=>aborted++,{once:true});
            }return request;
          };
          const observer=new PerformanceObserver(list=>{for(const e of list.getEntries()){
            if(e.name==='cell-page-reply-applied')applied++;
            if(e.name==='cell-page-reply-failed')failed++;
          }});observer.observe({type:'measure',buffered:false});
          try{
            const start=performance.now();while((completed<8||applied<8)&&performance.now()-start<30000)await new Promise(r=>setTimeout(r,50));
            const result={batches,completed,aborted,applied,failed,distinctPageConnections:databases.size,batchesPerConnection:[...databases.values()]};
            if(completed<8||applied<8||aborted||failed||databases.size!==1)throw Error(JSON.stringify(result));
            return result;
          }finally{IDBObjectStore.prototype.get=original;observer.disconnect();c.removeAttribute('data-cell-work-profile');c.removeAttribute('data-cell-performance-probe')}
        })()"#)?;
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
        println!("{}", browser.eval(&script)?);
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
        println!("{}", browser.eval(&script)?);
    }
    if let Some(error) = browser.trouble() {
        return Err(error);
    }
    Ok(())
}
