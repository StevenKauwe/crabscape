# Upward sand contact investigation

## Reproduction and scope

Baseline: `cee179bb020cdd268c7303f76ef64366b5638609`, Rust 1.99.1,
ARM64 Mac, repository release profile (`crabsim` opt-level 3, LTO).
Run `cargo run --offline --locked -p crabsim --release --example dig_frame_budget`.
The fixture is the real eastern coral sand shortcut, populated with creatures,
starting at x=17520 with four upgrades. It digs down, left, up, returns right,
then swims upward through the disturbed sand. All input goes through `Controls`.
Each timing sample covers two complete 120 Hz simulation ticks (a 60 Hz CPU
proxy). It excludes drawing, GPU, browser scheduling and storage latency; it is
not measured phone FPS. Three repetitions print positions and complete-state
hashes for each stage, outside the timed interval. No hard timing gate is used.

## Located cost

`world.rs` clears upward sediment obstruction at 120 Hz to preserve responsive
upward motion. `World::push_body` can search once per contacted grain, over several
passes. Each original `push_column` allocates and fills predecessor and load grids
for the maximum force radius even when the escape is nearby. Force 48 produces
99 by 99 entries in each grid, approximately 157 KB on a 64-bit native machine
(78 KB with 32-bit Wasm indices), plus a fresh FIFO queue for every search.

The change shares a lazily grown workspace across the grains of **one body push**.
It resets only loads visited by the preceding search and clears/reuses the queue.
It does not retain scratch in World, snapshots, hashes or saves. Search direction,
FIFO order, force limits, physics cadence and fallback scoops are unchanged.
The independent ChunkFlow transaction path is not changed by this patch.

## Verification

A test-only frozen copy of the original search is the behavioral oracle across
384 searches with different clipped origins, forces, directions and protected
bodies. Additional cases cover an exhausted dense search followed by an easy
clipped search. Backing-pointer/capacity checks prove repeated one-hop searches
reuse the buffers; their visited list stays below ten entries versus 9,801 slots.
This measures allocation reuse structurally, not with a system allocation profiler.

The baked source fingerprint includes actor-contact code and tests. These edits
cannot affect the offline bake, which calls the material kernel without actors.
Only its 32-byte source fingerprint is refreshed; all remaining decoded seed
bytes are verified identical. Runtime fingerprint verification remains enabled.

## Separate boundary result and limits

The existing `cell_transition_budget` baseline reported fresh canyon-down full
paged-tick median 3.068 ms, p95 3.240 ms, versus ordinary material-tick median
0.516 ms, p95 0.574 ms (30 samples). Page replies are in memory. This establishes
a CPU crossing surcharge, not the cause of a particular displayed frame drop.
Previous save/page waiting investigations remain separate; see
`solo-paging-stalls.md`. Do not claim this actor-contact patch fixes storage waits.

The live headless canyon frame probe could not start Chrome: no DevTools port
appeared within 60 seconds and its log was empty. Native `sample` initially could not inspect its own benchmark child in the
sandbox; an approved execution retry succeeded. Its 3-second, 1-ms sampling
capture has 555 leaf samples in inlined `push_body` search, 58 in `memset`,
and 576 in `tick_region_inner`. The diagnostic hashes consume samples too,
but are outside the benchmark timing intervals. Sampling supports targeting
the body search rather than attributing the entire stall to allocations.

## Intermediate scratch-only measurement

An A/B/B/A run with other agents' builds paused produced six ascents per version.
Original upward p95 range: 10.645–10.905 ms; maximum range: 21.761–22.649 ms.
Scratch-only p95 range: 9.686–10.144 ms; maximum range: 20.009–20.738 ms.
This is a repeatable improvement, but leaves 2 samples per 135-frame ascent over
16.667 ms (original 3–4). It is not a complete fix. Every stage's final position
and state hash matches the immutable original; upward final hash is
`399dd0b7f1d54866`. A preceding run overlapped compilation and is excluded from
this matched timing summary rather than hidden as favorable evidence.

## Live browser observation

An approved headless retry captured live build
`25cddf7f343c88fab0f563f9a3a1db1a1b845a48`, with an isolated synthetic 1,010-record,
9,974,795-byte save and 4x CDP CPU throttling. Its 454 frame samples had a
566.7 ms longest unchanged simulation tick. Rendering used SwiftShader software
Vulkan, not the Mac GPU: median 133.3 ms, p95 133.4 ms, maximum 200 ms. Those
render timings are dominated by a nonrepresentative backend and are **not** a
phone or hardware-rendered performance claim. This run is a separate observation,
not a before/after comparison with the native fix or proof of a specific wait cause.
The trace is `crab-frame-probe-71372` in the system temporary directory.

The longest pause was tick 1033, trace 20,663.0–21,229.7 ms. Telemetry shows
pending terrain throughout, with saving and reading alternating and revision
16→17. All 162 observed page replies were applied; the trace contains no stale
or failed-reply markers. Page-read totals reached 181.7 ms, final validation
106.1 ms, and publication waits 163.8 ms. This supports treating unresolved page
availability/publication scheduling as a separate investigation, not weakening
revision validation or claiming the grain-search patch addresses it.

## Search arithmetic follow-up

CPU sampling motivated replacing flat-index FIFO entries with `(i32, i32)`
coordinates. Queued nodes are already inside the clipped search rectangle;
neighbor bounds are checked before deriving flat indices by multiplication.
Reservation and target tests reuse those coordinates instead of repeatedly
converting indices with runtime division/modulo. Reconstruction retains the
original predecessor mapping. The unchanged frozen oracle checks ordering and
results. Queue entries remain 8 bytes on native ARM64 and increase from 4 to
8 bytes on wasm32; the queue remains bounded by search and body-push lifetime.

## Final matched result

After all competing Dragon builds/tests confirmed paused, a fresh A/B/B/A sequence
ran the immutable baseline and final coordinate-plus-scratch candidate, three
routes per invocation (six ascents per version, 135 samples each):

- Baseline upward p95: 10.486–10.590 ms; maximum: 21.417–21.789 ms.
- Candidate upward p95: 7.966–8.065 ms; maximum: 16.673–17.013 ms.
- Samples over 16.667 ms: 3 per baseline ascent, 1 per candidate ascent.
- Every stage position and complete state hash matches the baseline exactly.

The tail CPU cost drops approximately 23%; one over-budget CPU sample still
remains before adding rendering. This improves the reported symptom but does not
establish smooth phone frame pacing or close the separate border-paging issue.
Logs: `dig-final-a1.txt`, `dig-final-b1.txt`, `dig-final-b2.txt`, `dig-final-a2.txt`
in the system temporary directory. The final underwater release suite passes
243 tests, with 21 opt-in tests ignored; the three new search regressions pass.
