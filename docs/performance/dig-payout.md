# Digging experience without staging payments

At base `a656e3d7`, `Sim::dig_step` is called by the 120 Hz tick. When a local crab
has unpaid excavated blocks it allocated a temporary payment vector, then searched
the crab collection again for each payment. `earn` also cloned the whole tuning
value (a value copy, not a claimed heap allocation).

The award helper now borrows only one crab, tuning and the cue sink. Dig payouts
walk local crabs once, in the original order. Each debt is consumed immediately
before its award. Awarding does not read other crabs' debts, so clearing all debts
before awarding was unnecessary. Remote debt is unchanged. No scratch storage,
trait, new dependency, RNG call or save/hash field was added.

## Matched measurement

Mac ARM64, rustc 1.99.1, repository release profile, 2026-11-13 UTC:

```sh
RUSTC_WRAPPER= cargo test --offline --locked -p crabsim --release --lib \
  payout_tests::dig_payout_budget -- --ignored --nocapture
```

Five samples of 100,000 complete payout operations each, fixture creation outside
timing, every crab credited one block before each operation. The eight-crab
fixture includes one remote crab. The reference preserves the former payment
vector and lookup algorithm but uses the same borrowed award helper, isolating
batch dispatch rather than attributing a gain to the tuning copy.

| Fixture | Reference ns/op, range | Direct ns/op, range |
| --- | --- | --- |
| One local crab | 29.9–30.8 | 9.3–9.7 |
| Eight crabs, seven local | 129.8–133.6 | 60.0–62.2 |

Final state hashes match in every paired sample. This is a small absolute CPU
saving and removal of transient allocation in an active digging path, **not a
measured FPS improvement or a fix for border stutter**. Background load was not
controlled. No timing gate is imposed.

## Behavioral checks and limits

The regression compares the former two-stage algorithm with direct payouts using
mixed empty/single/multiple-level debts and a remote crab, then repeats to prove
exactly-once awards. It compares state hashes, ordered cues, debt and pending
level-up events. Existing economy tests cover invalid XP and unknown/remote IDs;
existing digging tests exercise actual tunneling and deterministic replay.

The existing `dig_frame_budget` example fails its `dig-left` route assertion on
unchanged base `a656e3d7` (`Vec2(17367.99, -4632.0)`), so it cannot establish a
complete matched frame benchmark for this change. Native/Wasm replay CI remains
required; local wasmtime is unavailable. No browser/frame-pacing claim is made.
