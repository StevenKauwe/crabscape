# Rust performance without fragile ownership

Use this alongside `CLAUDE.md` when changing simulation, rendering or background work. The goal is less measured
work with the same game rules, not fewer occurrences of `clone`, `collect`, `mem::take` or traits.

## A small workflow for each change

1. **Locate the cost.** Trace from a live caller and name its frequency: 120 Hz actor tick, 30 Hz material step,
   render frame, page crossing, save, join or explicit diagnostic. Exclude tests and isolated experiments from
   claims about gameplay. Profile CPU and allocations separately; inspect queue/wait time for asynchronous work.
2. **Make a matched baseline.** Record commit, command, fixture/seed, compiler/profile, target/device, sample count,
   warm/cold state and relevant background load. Use representative player counts and growing-world traversal.
   Repeat before/after runs; report absolute time and spread as well as percentage. Time complete operations in
   addition to microbenchmarks. Keep profiling/build overrides local and out of the shipping Wasm build.
3. **Change one cost and its ownership boundary.** Pass `&T`, `&mut T` or slices for the state a helper actually
   needs. Borrow independent fields separately; use safe slice splitting for disjoint mutable elements. Keep
   intentional before-update snapshots where order matters. A scratch buffer can live beside the stage that uses
   it, but must not become authoritative state or alter hashes/saves. Preallocate for a known bound, clear/refill
   when appropriate, and cap retained memory. Verify reuse rather than inferring it from a method name.
4. **Keep failure behavior.** Validate before publishing mutations. Preserve atomic commit/rollback and the last
   complete save on failed writes. Test rejected input, cold pages, cancellation and interrupted/repeated work
   where the change touches them. Use bounded background queues; consider a persistent worker for frequent jobs,
   but do not replace infrequent task spawning without evidence that it matters.
5. **Verify and report.** Add a focused regression test, then run the relevant repository checks from `CLAUDE.md`.
   Sim changes need deterministic replay/state comparisons and the native/Wasm checks. Preserve 120/30 Hz rates,
   seeded RNG consumption, stable ordering, conservation, regional ownership and checkpoint isolation. Rendering
   changes need a rendered check; headless sim tests cannot establish frame pacing. Report skipped or blocked
   checks. Use timing thresholds only on a calibrated runner with an established budget and noise allowance.

Keep ordinary cleanup small. Extract stages when they expose a clear responsibility and smaller borrow surface,
not to hit an arbitrary line count. A `Result`-returning boundary should make invalid state and failure explicit;
an enum should describe mutually exclusive lifecycle states rather than several contradictory flags.

## Traits and copies: inspect the contract

- Use narrow traits for genuine interchangeable behavior. A storage interface can support disk/browser adapters
  and deterministic failure tests when they share semantics; it should not erase meaningful async or durability
  differences. Accept `impl Iterator<Item = ...>` when a one-pass consumer need not own a collection.
- Use concrete types or enums for a fixed set of implementations. Generics provide static dispatch; trait objects
  enable runtime heterogeneity. Neither is a universal performance rule. Measure dispatch and generated-code size
  where they affect the workload, especially the size-tuned browser build.
- `Clone` may copy a buffer or merely share a handle. `clone_from` can reuse resources only when its implementation
  does so; a derived wrapper need not preserve its nested vectors' capacity. Test nested reuse if it is the reason
  for a change. Keep the equality and independence guarantees callers need.
- `mem::take`/`Option::take` move ownership without cloning the payload. Check restoration on every early return
  and whether repeatedly transferring a vector throws away reusable capacity. Do not replace them with unsafe
  pointers or interior mutability just to make the code look simpler.

## Measured starting point, not a permanent budget

The November 10, 2026 audit used immutable commit `0746b65`, Rust 1.99.1 and the repository's release profile on
an ARM64 Mac. An isolated checkout ran with the local compiler-cache wrapper disabled:

```sh
RUSTC_WRAPPER= cargo run --offline --locked -p crabsim --release --example cell_transition_budget
```

Each route has 30 samples. Ordinary material-tick medians across its six route/direction cases were
**0.566–0.739 ms**. Complete fresh cold-paged CPU tick medians were **1.999 ms** for opening-east, **2.540 ms**
for silt-east and **3.289 ms** for canyon-down; their p95 values were **2.811, 3.860 and 3.582 ms** respectively.
Page replies in this benchmark are supplied in memory. These figures exclude real storage/network latency,
drawing and GPU uploads. Background load was not controlled, so they are observations, not a regression limit,
proof of phone FPS or the savings from any proposed cleanup. Rerun on the current code before comparing changes.

Concrete candidates from that revision, to recheck before editing:

- `crates/sim/src/world.rs`, `runs_each`: a fresh ownership vector is built at three tick stages, including in
  `send_updates` before its solo early return. Start with the needless solo work; preserve changes between stages.
- The same file's body reservations build a list and then a list per crab. `World::reserve_other_crabs` replaces
  its buffer. Measure reusable storage without changing the per-crab exclusions or before-movement snapshot.
- `src/puppet.rs`, `animate`/`draw_order`: geometry, keys, indices, slots and the comparison copy are built each
  crab/frame. Cache invariant geometry and reuse bounded scratch before considering a larger rendering rewrite.
- `src/player.rs`, `simulate`: the presentation copies `Mover`/`Fighter` each frame. Their derived `Clone` at this
  revision does not reuse nested vector capacity through `clone_from`. A small presentation projection may help,
  but measure it: small copies need not be a meaningful share of the frame.

The full-world copies in `crates/sim/src/net/cell_join/input.rs` belong to an isolated correctness experiment, not the live
gameplay path. Hashing requested by `src/replay.rs` is a diagnostic cost, not a normal every-frame cost. Thread
creation for native page batches and saves is conditional; it is not one thread per tick. Coordinate with active
work on these files rather than assuming this historical list is still open.

## Primary references

- [The Rust Performance Book: profiling](https://nnethercote.github.io/perf-book/profiling.html): find the hot path;
  use CPU and heap tools appropriate to the platform. Instruments is one native macOS option.
- [Rustonomicon: splitting borrows](https://doc.rust-lang.org/nomicon/borrow-splitting.html): disjoint struct fields
  and mutable slices. Use the standard library's safe APIs; this repository does not permit new unsafe code.
- [Rust `Clone` contract](https://doc.rust-lang.org/std/clone/trait.Clone.html#method.clone_from): resource reuse is
  an implementation choice, not a guarantee of calling `clone_from`.
- [The Rust Book: trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html): concrete/generic types,
  closed enums, heterogeneous behavior and static versus dynamic dispatch.
