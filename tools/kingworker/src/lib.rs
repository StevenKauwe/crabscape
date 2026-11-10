//! The whole decision experiment runs in Rust, off the game's browser thread.
#[cfg(target_arch = "wasm32")]
mod browser {
    use wasm_bindgen::{JsCast, closure::Closure, prelude::wasm_bindgen};
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent, Response};
    fn send(scope: &DedicatedWorkerGlobalScope, value: serde_json::Value) {
        let _ = scope.post_message(&value.to_string().into());
    }
    async fn weights(scope: &DedicatedWorkerGlobalScope, kind: &str) -> Result<Vec<u8>, String> {
        let mut bytes = vec![];
        for i in 0..4 {
            let url = format!("/king-ddd/vendor/engine/weights/{kind}-v2-half.{i:02}.bin");
            send(
                scope,
                serde_json::json!({"state":"status","text":format!("Reading {kind} weights, part {} of 4…",i+1)}),
            );
            let r = wasm_bindgen_futures::JsFuture::from(scope.fetch_with_str(&url))
                .await
                .map_err(|_| "Weight download failed")?
                .dyn_into::<Response>()
                .map_err(|_| "Invalid weight response")?;
            if !r.ok() {
                return Err(format!("Weight download returned {}", r.status()));
            }
            let b = wasm_bindgen_futures::JsFuture::from(
                r.array_buffer().map_err(|_| "Could not read weights")?,
            )
            .await
            .map_err(|_| "Could not read weights")?;
            if js_sys::Uint8Array::new(&b).length() > 16 * 1024 * 1024 {
                return Err("Weight part exceeds memory budget".into());
            }
            bytes.extend(js_sys::Uint8Array::new(&b).to_vec());
        }
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("Weights exceed memory budget".into());
        }
        Ok(bytes)
    }
    #[wasm_bindgen(start)]
    pub fn start() {
        let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
        let target = scope.clone();
        let callback = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let Some(text) = event.data().as_string().filter(|s| s.len() <= 24_000_000) else {
                send(
                    &target,
                    serde_json::json!({"state":"error","error":"Invalid or oversized request"}),
                );
                return;
            };
            let reply = target.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result:Result<king_ddd::WorkResult,String>=async{let req:king_ddd::WorkRequest=serde_json::from_str(&text).map_err(|e|e.to_string())?;req.validate()?;let reg=req.columns.iter().any(|c|c.role==king_ddd::Role::Output&&c.kind==king_ddd::Kind::Number);let class=req.columns.iter().any(|c|c.role==king_ddd::Role::Output&&c.kind==king_ddd::Kind::Category);let reg=if reg{Some(weights(&reply,"regressor").await?)}else{None};let class=if class{Some(weights(&reply,"classifier").await?)}else{None};send(&reply,serde_json::json!({"state":"status","text":"Training the real TabICL models…"}));king_ddd::run(&req,reg,class,|g|send(&reply,serde_json::json!({"state":"generation","generation":g}))) }.await;
                match result {
                    Ok(result) => send(&reply, serde_json::json!({"state":"done","result":result})),
                    Err(error) => send(&reply, serde_json::json!({"state":"error","error":error})),
                }
            });
        });
        scope.set_onmessage(Some(callback.as_ref().unchecked_ref()));
        callback.forget();
        send(&scope, serde_json::json!({"state":"ready"}));
    }
}
