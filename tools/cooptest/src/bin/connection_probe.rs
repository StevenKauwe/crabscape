//! Isolated IndexedDB connection experiment; not production paging validation.
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
    let dir = std::env::temp_dir().join(format!("crab-connection-probe-{}", std::process::id()));
    let mut browser = Chrome::start_without_clipboard(
        "connection-probe",
        &dir,
        Options {
            fake_mic: false,
            slow: std::env::args()
                .nth(2)
                .unwrap_or_else(|| "1".into())
                .parse()
                .map_err(|_| "invalid throttle")?,
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
    // Keep initial renderer/asset startup out of the fixed-workload sequence.
    sleep(Duration::from_secs(3));
    let result = browser.eval(r#"(async()=>{
 const name='crab-connection-probe-'+Date.now();
 const open=()=>new Promise((resolve,reject)=>{const r=indexedDB.open(name,1);r.onupgradeneeded=()=>r.result.createObjectStore('pages');r.onerror=()=>reject(r.error);r.onsuccess=()=>resolve(r.result)});
 const finish=tx=>new Promise((resolve,reject)=>{tx.oncomplete=resolve;tx.onabort=()=>reject(tx.error||Error('aborted'))});
 let cached=null;
 const acquire=async reuse=>{if(reuse&&cached)return cached;const db=await open();db.onversionchange=()=>{db.close();if(cached===db)cached=null};if(reuse)cached=db;return db};
 const discard=db=>{db.close();if(cached===db)cached=null};
 const read=async(reuse,abort=false)=>{const db=await acquire(reuse);try{const tx=db.transaction('pages'),done=finish(tx),s=tx.objectStore('pages');const reads=Array.from({length:12},(_,i)=>s.get(i));if(abort)tx.abort();await done;return reads.map(r=>r.result)}catch(e){discard(db);throw e}finally{if(!reuse)discard(db)}};
 const db=await open();let tx=db.transaction('pages','readwrite'),done=finish(tx);for(let i=0;i<12;i++)tx.objectStore('pages').put(new Uint8Array(32768).fill(i),i);await done;db.close();
 const results=[];
 try{
  for(const reuse of [false,true,true,false]){
   const start=performance.now();let max=0;
   for(let n=0;n<64;n++){const t=performance.now(),rows=await read(reuse);if(rows.some((r,i)=>r.length!==32768||r[0]!==i||r[32767]!==i))throw Error('terrain mismatch');max=Math.max(max,performance.now()-t)}
   results.push({reuse,batches:64,pages_per_batch:12,elapsed_ms:performance.now()-start,max_batch_ms:max});if(cached)discard(cached);
  }
  let rejected=false;try{await read(true,true)}catch{rejected=true}if(!rejected||cached)throw Error('cancel did not discard');await read(true);
  cached.close();rejected=false;try{await read(true)}catch{rejected=true}if(!rejected||cached)throw Error('closed handle did not discard');await read(true);
  const reset=await open();tx=reset.transaction('pages','readwrite');done=finish(tx);tx.objectStore('pages').put(new Uint8Array(32768).fill(99),0);await done;reset.close();if((await read(true))[0][0]!==99)throw Error('reset not visible');
  await new Promise((resolve,reject)=>{const r=indexedDB.deleteDatabase(name);r.onsuccess=resolve;r.onerror=()=>reject(r.error);r.onblocked=()=>reject(Error('version change blocked'))});if(cached)throw Error('version change retained handle');
  return {results,cancellation_recovered:true,closed_handle_recovered:true,reset_visible:true,versionchange_closed:true,scope:'fixed page-read workload, not production paging or traversal'};
 }finally{if(cached)discard(cached);indexedDB.deleteDatabase(name)}
})()
"#)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
    );
    if let Some(error) = browser.trouble() {
        return Err(error);
    }
    Ok(())
}
