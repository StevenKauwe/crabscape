# Castle return readiness

The castle test reloads a stationary saved crab, verifies its restored position
and career, then requires the return walk within 12 seconds. The replacement
document's loading message, focus and castle marker do not prove that its
simulation has started. The clock is restored from the save too, so a nonzero
tick is not progress.

## Observed failure

The diagnostic run at PR head `0f15f51` reproduced the failure on Linux:
[job](https://github.com/StevenKauwe/crabscape/actions/runs/43082237614/job/130740868569),
[artifact](https://github.com/StevenKauwe/crabscape/actions/runs/43082237614/artifacts/13886590361).
Times below are milliseconds since replacement-document navigation.

- The old readiness predicate passed at 2361.
- Tick 17703 remained unchanged through 17459. Left controls reached the game's control state,
  focus remained on the canvas, and frames continued approximately every 780 ms.
- Seven batches of 12 terrain pages and one batch of 5 were installed before
  the first observed simulation progress. Successful reads and applications
  are recorded in User Timing; the page coordinates were not recorded.
- At 18294 the tick reached 17732. Subsequent frames advanced another 29 ticks
  each, moving the crab through x=436.9, 371.6, 306.1 and 231.2 by 20640.
- These last observations were captured after the failure screenshot while
  left input remained held. They explain continued progress, but are not a
  passing acceptance result and screenshot capture can affect painting.

Required terrain/memory paging can stop a simulation tick. The browser loads
bounded batches asynchronously and applies replies on a later game frame.
Slow software rendering amplifies the initialization interval. This evidence
supports distinguishing initialization from the movement test; it does not
establish a rendering or storage performance fix.

## Test contract

Keep the original 120-second reload deadline. After the replacement document
meets the existing readiness conditions, record a numeric baseline tick and
require an increase before sending movement input. A restored, missing or
unchanged clock is insufficient. Losing readiness resets the baseline.

The position and career restoration assertions remain. The return movement
still has 12 seconds, with the same target and reach. A target first observed
after that deadline fails, even if a browser command was slow. A later paging
or rendering stall still fails movement; readiness is not retried during it.

Bounded frame, input, simulation, storage and timing histories remain in CI
artifacts for passing return walks and failures. Attribute publishers have
different cadences, and async timing measures are wall latency rather than
necessarily synchronous CPU work. No production loading, physics, cache or
rendering behavior changes in this correction.
