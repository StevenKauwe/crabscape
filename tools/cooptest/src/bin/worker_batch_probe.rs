//! Controlled worker-message comparison while the game continues rendering.
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
    let dir = std::env::temp_dir().join(format!("crab-worker-batch-probe-{}", std::process::id()));
    let mut browser = Chrome::start(
        "batch-probe",
        &dir,
        Options {
            fake_mic: false,
            slow: 4.0,
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
    let result = browser.eval(r#"(async()=>{
      const worker=new Worker(new URL('cellworker/worker.js',location.href),{type:'module'});
      let id=0;
      try {
        await new Promise((resolve,reject)=>{worker.onmessage=e=>e.data.state==='ready'?resolve():reject(e.data);worker.onerror=reject});
        const records=Array.from({length:128},(_,i)=>Uint8Array.from({length:32768},(_,j)=>(j*31+Math.floor(j/13)+i)%256));
        const request=(bytes,batch)=>new Promise((resolve,reject)=>{const next=++id;
          worker.onmessage=e=>{const d=e.data;if(d.state==='done'&&d.id===next)resolve(new Uint8Array(d.bytes));else reject(d)};
          worker.postMessage({id:next,bytes:bytes.buffer,batch},[bytes.buffer]);
        });
        const pack=pages=>{const output=new Uint8Array(4+pages.reduce((n,p)=>n+4+p.length,0));const v=new DataView(output.buffer);v.setUint32(0,pages.length,true);let at=4;
          for(const p of pages){v.setUint32(at,p.length,true);at+=4;output.set(p,at);at+=p.length}return output};
        const unpack=bytes=>{const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);let at=4;const pages=[];
          for(let i=0;i<view.getUint32(0,true);i++){const n=view.getUint32(at,true);at+=4;pages.push(bytes.slice(at,at+n));at+=n}if(at!==bytes.length)throw Error('bad frame');return pages};
        await request(records[0].slice(),false);
        let expected=null;const results=[];
        for(const batch of [false,true,true,false]){
          let running=true,previous=null;const frames=[];
          function frame(at){if(!running)return;if(previous!==null)frames.push(at-previous);previous=at;requestAnimationFrame(frame)}requestAnimationFrame(frame);
          const begin=performance.now(),output=[];
          if(batch){for(let i=0;i<records.length;i+=16)output.push(...unpack(await request(pack(records.slice(i,i+16)),true)))}
          else{for(const record of records)output.push(await request(record.slice(),false))}
          const elapsed=performance.now()-begin;running=false;
          if(expected){if(output.length!==expected.length||output.some((p,i)=>p.length!==expected[i].length||p.some((b,j)=>b!==expected[i][j])))throw Error('different compression bytes')}
          else expected=output;
          frames.sort((a,b)=>a-b);results.push({batch,records:records.length,raw_bytes:records.reduce((n,p)=>n+p.length,0),elapsed_ms:elapsed,frames:frames.length,max_frame_ms:Math.max(0,...frames),p95_frame_ms:frames[Math.floor(frames.length*.95)]??null,frames_over_60:frames.filter(n=>n>60).length});
        }
        return {same_compressed_bytes:true,cpu_throttle:4,order:'serial,batch,batch,serial',results};
      } finally {worker.terminate()}
    })()"#)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
    );
    if let Some(error) = browser.trouble() {
        return Err(error);
    }
    Ok(())
}
