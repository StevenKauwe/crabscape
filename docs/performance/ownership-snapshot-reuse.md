# Reuse ownership snapshot storage between simulation ticks

Baseline: `bc286547`, Rust 1.99.1, native Apple Silicon. The live actor tick
(`Sim::tick_region_inner`, 120 Hz) allocated a fresh `Vec<bool>` before checking
claw hits, then reused it for the enemy-step and outgoing-update stages. With a
nonempty enemy roster this still required at least one allocation every tick.

The buffer now survives between ticks, but its **values do not**. Each existing
stage still clears and rebuilds its ownership decisions, preserving handovers
between stages. It is cleared before returning to `Sim`. No serialization or
state hash includes it. `mem::take` temporarily lends only disposable scratch to
helpers; no authoritative state is removed. There are no early returns between
checkout and restoration. Capacity above 4,096 bytes is discarded at tick end so
an exceptional roster does not permanently grow the cache.

## Regression proof

`ownership_scratch_reuses_storage_without_changing_replay` compares 120 ticks
against the previous fresh-buffer path, switching solo → host → guest. Every
state hash and outgoing packet matches. The warm buffer retains its pointer and
capacity throughout (no growth or replacement); clearing a deliberately oversized
buffer and recovering reuse is also checked. The existing stage-refresh test
continues to verify changes of authority are observed.

This removes the recurring ownership-vector allocation after warmup for ordinary
rosters. It does **not** eliminate other tick allocations and is not a measured
phone-FPS claim. No new dependency, gameplay tuning or rendering changes.

Commands:

```sh
RUSTC_WRAPPER= cargo test --offline --locked -p crabsim --lib world::tests
RUSTC_WRAPPER= cargo clippy --offline --locked -p crabsim --all-targets -- -D warnings
```

Native/Wasm determinism and the remaining browser integration checks are required
in CI; local Wasmtime is not installed.

Local checks (November 14 UTC): 9 world unit tests and 6 property-test groups
(`PROPTEST_CASES=16`) pass. Native all-target crabsim clippy and wasm32 library
clippy pass with warnings denied; formatting and whitespace checks pass.
