//! Install large fixtures while the game is unloaded. Keep each WebDriver
//! message and IndexedDB transaction bounded, then publish the head last.
use serde_json::{Value, json};

pub(super) fn install(
    database: &str,
    fixture: &Value,
    mut eval: impl FnMut(&str) -> Result<Value, String>,
) -> Result<(), String> {
    let records = fixture["records"]
        .as_array()
        .ok_or("fixture lacks records")?;
    let mut write = |rows: &[Value], clear: bool| {
        let rows = serde_json::to_string(rows).map_err(|e| e.to_string())?;
        let script = format!(
            r#"new Promise((resolve,reject)=>{{
          const open=indexedDB.open({database:?},1);
          open.onerror=()=>reject(String(open.error));
          open.onblocked=()=>reject('fixture database open blocked');
          open.onsuccess=()=>{{
            const db=open.result;
            try {{
              const tx=db.transaction('world','readwrite'), store=tx.objectStore('world');
              tx.onabort=()=>{{db.close();reject(String(tx.error));}};
              tx.oncomplete=()=>{{db.close();resolve(true);}};
              if ({clear}) store.clear();
              for(const [key,value] of {rows}) {{
                const raw=atob(value), bytes=new Uint8Array(raw.length);
                for(let i=0;i<raw.length;i++) bytes[i]=raw.charCodeAt(i);
                store.put(bytes,key);
              }}
            }} catch(e) {{db.close();reject(String(e));}}
          }};
        }})"#
        );
        eval(&script).map(drop)
    };
    for (batch, rows) in records.chunks(16).enumerate() {
        write(rows, batch == 0).map_err(|e| format!("large fixture records batch {batch}: {e}"))?;
    }
    write(
        &[
            json!(["reset_epoch", fixture["epoch"]]),
            json!(["active", fixture["head"]]),
        ],
        records.is_empty(),
    )
    .map_err(|e| format!("large fixture head publication: {e}"))
}
