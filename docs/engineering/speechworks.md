# Shared on-device speech

Spoken reports and voice-chat transcripts use
[speechworks](https://github.com/StevenKauwe/speechworks), the same Rust inference
and browser worker used by Agency Platform. CrabScape owns microphone capture,
voice attribution, its existing utterance segmentation and transcript display.
Speechworks owns model loading, transcription and GPU/CPU selection. No speech
model or layer dimensions are compiled into the game.

`tools/speechworks-revision.txt` pins a full public commit. During
`tools/build-web.sh`, `tools/install-speechworks.sh` downloads that source and
invokes its shell/Rust installer. Missing model files are downloaded into the
build cache and verified against their pinned SHA-256 and size. The finished
site contains the runtime, model manifest and weights beneath
`web/listen/runtime/<revision>-<manifest-sha256>/`. Runtime requests stay on the site's origin;
speech never goes to a server. Generated files and model weights are not
committed to the game repository.

`web/listen/worker.js` fetches the build's revision stamp without using a cached
response, then imports the runtime at its immutable URL. Changing model policy
changes this URL even when the library revision stays the same. Its JavaScript,
WebAssembly and manifest therefore come from one revision. The game's Rust
listener sends `transcribe` messages with a job id and a report-priority flag.
The shared worker processes clips serially, and each `result` or `error` resolves
the matching job. An initialization failure releases every waiting caller and
discards the worker, allowing the next recording to try again.

The manifest's final-model selection is used for complete report/chat clips.
Model and backend policy belongs in that manifest and the worker options, not
in `src/listener.rs`. The shared library also supports live dictation sessions;
the game's existing transcript flow continues to submit complete utterances.

## Build and update

- `tools/install-speechworks.sh` installs speech alone, without rebuilding Bevy.
- `tools/build-web.sh` builds the game and all its workers. It validates the
  speech WASM without relaxed SIMD, like the other game modules.
- `tools/test-speechworks-install.sh` checks pinned-source handling and makes
  sure a failed/partial install cannot publish a new version stamp. It does
  not substitute for the speech library's real-audio inference tests.
- To update, review a speechworks commit, put its full SHA in
  `tools/speechworks-revision.txt`, rebuild, and exercise a spoken report and
  voice chat in a browser. Upstream tests cover CPU/GPU inference and streaming.

Set `SPEECHWORKS_SOURCE` to a clean local checkout of the pinned revision to
avoid downloading source. `SPEECHWORKS_MODEL_DIR` selects a reusable model cache;
the installer still verifies every model. `SPEECHWORKS_MANIFEST` selects a custom
pinned model manifest instead of the library's default. `CARGO_TARGET_DIR` selects the game
build/cache root; the speech build has its own subdirectory. The generated
manifest may be inspected to see the exact models and hashes that will ship.
