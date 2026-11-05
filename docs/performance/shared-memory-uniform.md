# Shared memory lighting uniforms

MemoryMaterial previously received the same lighting and exploration-grid values on every
asset each frame. Bevy consequently rebuilt their uniform buffers and bindings. A single
144-byte render-world uniform now holds those values. Static per-material color and texture
bindings remain intact; the shader equations and simulation are unchanged.

## Regression coverage

Memory and lantern system tests verify the actual shared grid/light values and verify that
updating them does not emit per-material modification events (including 64 memory materials).
The render upload runs after extracted commands and before material preparation. WebGL2 uses
an ordinary uniform binding, not storage buffers. Native tests/clippy and Wasm compilation pass.

## Local browser measurement, 2026-11-05

Apple M2 Max, Chrome ANGLE Metal, 4x CDP CPU throttling and CPU sampling. Both builds use
release optimization plus retained function names. Each capture runs the existing 60-second
canyon rotation/paddle/dive route from a synthetic 9,851,246-byte save (924 spatial records).
No player save was used. No concurrent compilation ran during captures.

Reproduce against independently served before/after builds:

```sh
CRABSCAPE_PROFILE_GPU=1 cargo run -p cooptest --bin frame_probe -- \
  --url http://127.0.0.1:PORT/ --route canyon --large-save --slow 4 --cpu
```

The probe records frame gaps, CPU profile, and WebGL allocation/upload call counts in its
printed temporary trace directory. Method wrappers and CPU sampling add measurement overhead.

| Metric | Before b80cabe | Candidate 206d176 |
| --- | ---: | ---: |
| Frames | 1,146 | 1,669 |
| createBuffer calls | 568,311 | 247,356 |
| createBuffer calls/frame | 495.9 | 148.2 |
| MemoryMaterial prepare inclusive sampled time | 5.627 s | 0.022 s |
| Median frame interval | 50.0 ms | 33.3 ms |
| p95 interval | 66.7 ms | 50.1 ms |
| Maximum interval | 83.4 ms | 66.8 ms |
| Intervals above 60 ms | 306 | 21 |

Both captures completed without reported browser/shader errors. No textures were allocated
in either measured interval. CPU totals above count each sample once when any ancestor is
MemoryMaterial's prepare_erased_assets function.

These are single diagnostic captures, not a calibrated timing gate. Input is driven by wall
clock, so different frame pacing changes the traversed path and visible workload. The direct
allocation reduction and near-elimination of material preparation support the mechanism;
timing differences are supporting evidence, not a matched-route FPS promise. This does not
establish physical iPhone performance, co-op latency, or removal of every transition hitch.
Remaining buffer allocations and simulation work are not addressed by this change.
