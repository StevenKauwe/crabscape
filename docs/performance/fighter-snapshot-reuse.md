# Fighting snapshots reuse their buffers

`src/player.rs::simulate` copies each simulated crab's `Fighter` into the
presentation entity once per render frame. Calling `clone_from` did not avoid
allocations: the derived `Clone` implementation replaced the entire object,
including `venom` and the current swing's `hits` vectors.

`Fighter` and `Swing` now implement the same deep-copy contract with field-wise
`clone_from`. A warm destination retains both buffers while their lengths fit.
Ending a swing still drops it; starting another can allocate. There is no cache
of obsolete swings, new authoritative state, changed serialization, RNG use,
ordering, or game rule. The explicit implementations cost some boilerplate;
the regression compares every field and serialized bytes, and verifies nested
capacity/pointer reuse and independence, including empty lists and ending swings.

## Reproduce

Base: `71c05d0d`, Rust 1.99.1, ARM64 macOS, repository release profile.

```sh
CARGO_BUILD_JOBS=2 RUSTC_WRAPPER= cargo test -p crabsim --release --offline \
  --test fighter_clone frame_snapshot_cost -- --ignored --nocapture
```

Seven paired passes copy eight warm fighting snapshots over 100,000 frames per
pass. Each fixture has three venom doses and three already-hit targets. The
baseline explicitly uses `*target = source.clone()`, the previous default
`Clone::clone_from` behavior, in the same binary as the new implementation.

| Snapshot copy | Median | Observed range |
|---|---:|---:|
| Replace/deep clone | 47.7 ns | 47.1–49.2 ns |
| Reuse nested buffers | 11.6 ns | 11.3–13.1 ns |

The useful structural change is **two vector allocations to zero** per warm
snapshot in this fixture. Empty vectors did not allocate before; ordinary
unpoisoned/non-attacking crabs do not have this two-allocation saving. These are
microbenchmark results, not end-to-end frame timings or phone FPS. Other work
was active on the Mac, and even at eight crabs this is less than a microsecond
of measured copy time per frame. This is allocation hygiene, not a fix for
cell-border stalls. The separate `Mover` snapshot is unchanged.
