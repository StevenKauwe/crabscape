//! Compression for a cellular checkpoint, independent of the Bevy game.
//!
//! In a browser this runs in a dedicated Web Worker. Input and output are
//! transferred as ArrayBuffers, so the worker's long compression step does
//! not run on the frame thread. This module does not save or install worlds.

mod batch;

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
    if !matches_original(&compressed, bytes) {
        return Err("compressed checkpoint failed verification");
    }
    Ok(compressed)
}

/// Verify before reporting success, using a fixed buffer rather than keeping a
/// second uncompressed checkpoint in memory. The host keeps its previous
/// save if compression or verification fails.
fn matches_original(compressed: &[u8], expected: &[u8]) -> bool {
    use miniz_oxide::{
        DataFormat, MZFlush, MZStatus,
        inflate::stream::{InflateState, inflate},
    };
    let mut state = InflateState::new_boxed(DataFormat::Zlib);
    let mut buffer = vec![0; 32 * 1024];
    let (mut read, mut written) = (0usize, 0usize);
    loop {
        let result = inflate(&mut state, &compressed[read..], &mut buffer, MZFlush::None);
        read += result.bytes_consumed;
        let end = written + result.bytes_written;
        if end > expected.len() || buffer[..result.bytes_written] != expected[written..end] {
            return false;
        }
        written = end;
        match result.status {
            Ok(MZStatus::StreamEnd) => {
                return read == compressed.len() && written == expected.len();
            }
            Ok(_) if result.bytes_consumed != 0 || result.bytes_written != 0 => {}
            _ => return false,
        }
    }
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
            let batched = Reflect::get(&data, &"batch".into())
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let packed = if batched {
                super::compress_batch(&input)
            } else {
                super::compress(&input)
            };
            match packed {
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

    #[test]
    fn detects_corruption_truncation_extra_bytes_and_wrong_contents() {
        let input: Vec<_> = (0..200_000)
            .map(|n| ((n * 31 + n / 13) % 256) as u8)
            .collect();
        let packed = compress(&input).unwrap();
        assert!(matches_original(&packed, &input));
        assert!(!matches_original(&packed, &input[..input.len() - 1]));
        assert!(!matches_original(&packed[..packed.len() - 1], &input));
        let mut damaged = packed.clone();
        damaged[5] ^= 1;
        assert!(!matches_original(&damaged, &input));
        damaged = packed.clone();
        damaged.push(0);
        assert!(!matches_original(&damaged, &input));
        assert!(matches_original(&compress(&[]).unwrap(), &[]));
    }
}

pub fn compress_batch(frame: &[u8]) -> Result<Vec<u8>, &'static str> {
    let records = batch::raw_records(frame)?;
    let packed = records
        .into_iter()
        .map(compress)
        .collect::<Result<Vec<_>, _>>()?;
    batch::pack(&packed.iter().map(Vec::as_slice).collect::<Vec<_>>())
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    #[test]
    fn batch_matches_individual_verified_compression() {
        let records: Vec<Vec<u8>> = (0..16)
            .map(|i| (0..(i * 999)).map(|n| (n % 251) as u8).collect())
            .collect();
        let frame = batch::pack(&records.iter().map(Vec::as_slice).collect::<Vec<_>>()).unwrap();
        let packed = compress_batch(&frame).unwrap();
        for (raw, packed) in records.iter().zip(batch::unpack(&packed).unwrap()) {
            assert_eq!(compress(raw).unwrap(), packed);
        }
    }
}
