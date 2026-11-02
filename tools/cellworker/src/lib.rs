//! Compression for a cellular checkpoint, independent of the Bevy game.
//!
//! In a browser this runs in a dedicated Web Worker. Input and output are
//! transferred as ArrayBuffers, so the worker's long compression step does
//! not run on the frame thread. This module does not save or install worlds.

/// Bound work and memory for a single checkpoint. Callers must also limit the
/// number of pending messages; Web Workers queue messages before dispatch.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Compress the raw checkpoint payload using the format of the save codec.
pub fn compress(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("checkpoint is too large");
    }
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(bytes, 1);
    if compressed.len() > MAX_BYTES {
        return Err("compressed checkpoint is too large");
    }
    Ok(compressed)
}

#[cfg(target_arch = "wasm32")]
mod web {
    use js_sys::{Array, ArrayBuffer, Object, Reflect, Uint8Array};
    use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

    fn field(obj: &Object, name: &str, value: &JsValue) {
        let _ = Reflect::set(obj, &name.into(), value);
    }

    fn error(scope: &DedicatedWorkerGlobalScope, id: u32, message: &str) {
        let response = Object::new();
        field(&response, "state", &"error".into());
        field(&response, "id", &id.into());
        field(&response, "error", &message.into());
        let _ = scope.post_message(&response);
    }

    #[wasm_bindgen(start)]
    pub fn start() {
        let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
        let reply = scope.clone();
        let callback = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let data = event.data();
            let id = Reflect::get(&data, &"id".into())
                .ok()
                .and_then(|v| v.as_f64())
                .filter(|&n| n.is_finite() && n >= 1.0 && n <= u32::MAX as f64 && n.fract() == 0.0)
                .map(|n| n as u32);
            let Some(id) = id else {
                error(&reply, 0, "invalid request id");
                return;
            };
            let Ok(buffer) =
                Reflect::get(&data, &"bytes".into()).and_then(|v| v.dyn_into::<ArrayBuffer>())
            else {
                error(&reply, id, "checkpoint must be an ArrayBuffer");
                return;
            };
            if buffer.byte_length() as usize > super::MAX_BYTES {
                error(&reply, id, "checkpoint is too large");
                return;
            }
            // Copy from the transferred JS buffer into the worker's Wasm
            // memory; Wasm linear memory itself must never be transferred.
            let input = Uint8Array::new(&buffer).to_vec();
            match super::compress(&input) {
                Ok(packed) => {
                    let output = Uint8Array::from(packed.as_slice());
                    let buffer = output.buffer();
                    let response = Object::new();
                    field(&response, "state", &"done".into());
                    field(&response, "id", &id.into());
                    field(&response, "bytes", &buffer);
                    let transfer = Array::new();
                    transfer.push(&buffer);
                    if reply
                        .post_message_with_transfer(&response, &transfer)
                        .is_err()
                    {
                        error(&reply, id, "could not send compressed checkpoint");
                    }
                }
                Err(message) => error(&reply, id, message),
            }
        });
        scope.set_onmessage(Some(callback.as_ref().unchecked_ref()));
        callback.forget();
        let ready = Object::new();
        field(&ready, "state", &"ready".into());
        let _ = scope.post_message(&ready);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zlib_round_trips_and_matches_the_save_format() {
        let input: Vec<_> = (0..200_000)
            .map(|n| ((n * 31 + n / 13) % 256) as u8)
            .collect();
        let encoded = compress(&input).unwrap();
        assert_eq!(
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&encoded, input.len()).unwrap(),
            input
        );
    }

    #[test]
    fn rejects_oversized_input_before_compression() {
        assert_eq!(
            compress(&vec![0; MAX_BYTES + 1]),
            Err("checkpoint is too large")
        );
    }
}
