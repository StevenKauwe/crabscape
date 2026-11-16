# Third-party notices

Speechworks' dictation/session layer, browser integration and asset tooling are
Copyright (c) 2026 Steven Kaai Kauwe, licensed under MIT (LICENSE).

The inference implementation derives from [wavo](https://github.com/vladkens/wavo),
revision `06dfe6642db8a2f9159ad0f23719f23276431a84`. Its MIT copyright notices cover
vladkens, the transcribe.cpp authors, and the ggml authors. Their full license is
retained in `crates/inference/LICENSE-WAVO` and distributed in `licenses/wavo.txt`.
Speechworks adds browser-safe asynchronous GPU operations, uniform parameters,
in-memory model loading, bounded inference, and device-loss CPU recovery.

The Moonshine Streaming implementation follows the model equations and GGUF
contract in handy-computer/transcribe.cpp, revision
`adfc50210a3f026a655a3e19937d90ef2bf0728c` (MIT, copyright 2026 The
transcribe.cpp authors). Its full license is in `licenses/transcribe-cpp.txt`.
Speechworks implements its own Rust and WGSL bounded per-layer history caches;
no C/C++ inference library is linked or downloaded by the application.

The GPU portability design follows Steven Kauwe's BSD-3-Clause tabicl-anywhere;
the speech kernels are derived from wavo, not the TabICL model.

Silero VAD inference derives from `silero-vad-pure` 0.1.1, MIT, maintained here
as `speechworks-vad`. It retains the previous 64 samples (32 at 8 kHz) required
by the official streaming wrapper, and removes the optional Accelerate dependency.
Original notices and model provenance are retained in `crates/vad`.
The embedded Silero v6.2 model is MIT, from Silero Team.
Moonshine tiny/base and English Streaming Small/Medium weights are MIT, from Useful Sensors; the default
manifest pins handy-computer's GGUF conversions by immutable revision, byte
length and SHA-256. Model files are downloaded at build/install time and served
by the consuming application. Code and model licenses are separate; applications
must review the license of any replacement models they choose.

Other Rust dependencies retain their own licenses as listed in their Cargo
packages. Cargo.lock records the exact dependency versions used for this build.

## Optional threaded WASM adapter

The optional `wasm-threads` build uses wasm-bindgen-rayon 1.3.0 (Apache-2.0).
`tools/wasm-rayon-worker.js` adapts its `workerHelpers.no-bundler.js`, copyright
2022 Google Inc., retaining the upstream notice. Changes use same-origin module
workers instead of blob workers and add bounded startup/error cleanup. The full
license is in `licenses/wasm-bindgen-rayon.txt`. This adapter is not used by the
ordinary non-isolated WASM runtime.
