// Generated-module loader only. All data, learning and tournament logic is Rust.
fetch("./kingworker-version.txt", {cache:"no-store"})
 .then(r=>r.ok?r.text():"")
 .then(v=>v.trim()?"?v="+encodeURIComponent(v.trim()):"")
 .then(q=>import("./kingworker.js"+q).then(m=>m.default({module_or_path:new URL("./kingworker_bg.wasm"+q,self.location.href)})))
 .catch(error=>self.postMessage(JSON.stringify({state:"failed",error:String(error)})));
