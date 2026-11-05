# Creature scent rendering cost

Scent presentation now calculates perception once per creature per frame, sharing
that result across its puppet parts. The cache is cleared each frame so changing
sight, scent, position or enemy presence takes effect immediately. Atlas completion
moves the prepared pixels into an image with the original sampling metadata instead
of cloning and discarding the source pixels. Blur appearance and preparation's
16-row frame budget are unchanged. Simulation, terrain, saves and assets are unchanged.

Measured locally on macOS with the optimized `art` profile:

| Headless CPU measurement | Before | After |
| --- | ---: | ---: |
| 32 creatures / 2,048 parts, median warmed App update | 0.250 ms | 0.174 ms |
| Largest mean of 20 ten-update batches | 0.333 ms | 0.204 ms |
| Complete a 1,024 × 1,024 RGBA atlas, median | 0.096750 ms | Below timer resolution |
| Atlas completion p95 | 0.133792 ms | 0.000042 ms |

The atlas comparison excludes preparation/allocation of the output and GPU upload.
It removes a 4 MiB source copy; it does not eliminate texture upload cost. The scene
is a synthetic many-part stress case, not a representative phone frame budget.
These measurements establish reduced CPU overhead, not a diagnosis or complete fix
for intermittent gameplay stutter. Timing tests are ignored informational benchmarks,
not machine-dependent CI gates. Functional tests cover restoring normal art when
perception changes, blur preparation, and preserving image metadata and pixel ownership.

Reproduce with:

```sh
cargo test -p crabscape --profile art creature_art::tests
cargo test -p crabscape --profile art creature_art::tests::benchmark -- --ignored --nocapture
```
