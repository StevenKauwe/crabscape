# Full browser frames during ascent and descent

The cell-transition stutter report remains open. A remap microbenchmark does not
measure the time a player waits for a frame or for the simulation to advance.
This probe drives a real headless Chrome for 30 seconds: hold right, repeatedly
paddle upward for 15 seconds, then dive for 15 seconds. It uses the `hulk` cheat
in a fresh temporary browser profile; it never opens a player's existing profile.
The route derives from the reported repeated-jump / rapid-descent reproduction.
It is an opening-area stress route, not coverage of every deep canyon boundary.

```sh
CRABSCAPE_PROFILE_GPU=1 cargo run -p cooptest --bin frame_probe -- --url https://crabscape.com/
# Explicit calibrated local guard (milliseconds between rendered frames):
CRABSCAPE_PROFILE_GPU=1 cargo run -p cooptest --bin frame_probe -- --max-gap-ms 60
# Investigate a failure; CPU profiling adds overhead:
CRABSCAPE_PROFILE_GPU=1 cargo run -p cooptest --bin frame_probe -- --cpu
```

The command prints its unique temporary artifact directory. `frames.json` records
every animation-frame interval, position, available regional tick/revision,
cell-work measures, long tasks, build revision, renderer and CPU throttle.
On a passing run, `summary.json` reports median, p95, maximum and frames over 60 ms. `--cpu` adds
`cpu.json` for DevTools. Solo mode currently exposes no regional tick/revision;
those fields are null, not evidence of stalled simulation. Rendering can continue
while gameplay waits. Frame intervals alone cannot establish input latency or
all simulation pauses.

The probe must travel beyond x=1,200, rise above y=2,000 and return below y=500.
A stationary scene cannot pass. It fails on page errors, insufficient samples,
incomplete travel or an explicitly supplied maximum-gap budget. Tests verify
that one 233 ms hitch fails a 60 ms budget even when median pacing is healthy,
and that a stationary scene fails regardless of FPS.

No machine-independent timing gate is added to CI. A 60 Hz browser normally
returns a frame every 16.7 ms, so a 10 ms *frame interval* gate is inappropriate;
a 10 ms transition-work budget is a different metric. Calibrate an explicit
frame-gap limit on the same renderer, machine, build mode and route, recording
cold versus warm runs. Default Chrome acceptance rendering uses SwiftShader;
set `CRABSCAPE_PROFILE_GPU=1` for hardware and verify the recorded renderer.
`--slow N` throttles CPU only and does not emulate a phone's GPU or storage.

## Initial evidence, November 5

Live build `fb8226894965418bea90c27f6e46b48fcb6ee9a4`, local Mac, hardware Chrome:

- Existing two-client travel profiling captured ordinary regional moves around
  12–17 ms and a 183–300 ms simulation pause during shared-to-separated topology
  change. No frame gap exceeded that profiler's 60 ms reporting threshold.
- A full-frame solo ascent/descent capture recorded 1,814 intervals: median
  16.7 ms, p95 16.8 ms, maximum 233.3 ms, with a 243 ms long task.
  The CPU profile sampled about 221 ms inside WebGL `getProgramParameter` at
  the same episode. This points to program setup for that hitch, not a proven
  terrain-remapping cause. A subsequent capture did not repeat the long stall.
- A 4x CPU-throttled run recorded median 133.4 ms, p95 183.3 ms and maximum
  200.1 ms. This demonstrates why the renderer and throttle must accompany any
  reported timing; it is not a phone-performance measurement.
- An earlier height-band route stopped near x=269 while rendering continued.
  Repeated ascent/descent completed the route, so that stopped position was
  not proof of a frozen simulation and is excluded from transition results.

No gameplay fix follows from these measurements yet. Claude's unfinished
cellular-compute spike covers remapping, archive copying and flow computation;
this tool leaves those files and its existing browser driver unchanged.

The committed probe was exercised without CPU profiling on the same live build:
1,812 intervals, maximum 16.8 ms, route x=1,784 / peak y=4,951 / final y=-192,
ANGLE Metal on Apple M2 Max. It passed an explicit local 60 ms gap budget.
This warmed run does not erase the earlier captured 233 ms hitch.

## Canyon rotation and explored-world coverage

```sh
# A disclosed synthetic start in the canyon up-current:
CRABSCAPE_PROFILE_GPU=1 cargo run -p cooptest --bin frame_probe -- --route canyon --cpu
# The same route with the existing large archive / fully explored memory fixture:
CRABSCAPE_PROFILE_GPU=1 cargo run -p cooptest --bin frame_probe -- --route canyon --large-save --max-gap-ms 60
```

The canyon route uses a new temporary browser profile, generates a synthetic save
at (6250, -5000), leaves the game before installing it, then requires that save
to load. It never imports or changes a player's save. The native fixture must
match the game being measured; a failed load fails the probe. Artifacts label the
route and synthetic fixture, its save version, encoded size and record count.
`--large-save` requires `--route canyon` and uses the existing save regression's
extensively explored memory and archived terrain.

After `hulk`, Swimming level 99 and `feast`, the 60-second route alternates
20 seconds of clockwise rotation/repeated paddling, 20 seconds of diving, and
20 seconds of counter-clockwise rotation/repeated paddling. The current can carry
the crab out of the canyon: this is a scripted stress route, not a guarantee of
60 seconds in the up-current. Acceptance requires observed depth below -3000,
travel above -2000 and over 300 horizontal units. Opening-only motion and a
stationary deep scene cannot pass. The existing single-long-frame budget guard
still applies. Solo ticks remain unavailable, so this does not establish every
simulation or input-latency stall from FPS alone.

Initial exploratory captures on live `5a60e70`, ANGLE Metal / M2 Max, with CPU
profiling, did not reproduce the severe slowdown. The fresh fixture recorded
3603 intervals (median/p95 16.7 ms, max 33.3 ms), and the large fixture 3618
(median/p95 16.7 ms, max 33.4 ms); neither had long tasks. Both travelled from
roughly y=-4250 to the surface. These synthetic cases do not prove every older
save, deep location or slower device is smooth. Temporary evidence is under
/tmp/crab-canyon-frames-36591 and /tmp/crab-canyon-frames-42041.

The promoted CLI also passed the large-fixture route and an explicit local 60 ms
budget: 3601 frames, max 33.4 ms, 772 spatial records, 9,340,676 encoded bytes,
y=-4409.7..4951.7. Its trace is crab-frame-probe-45673 in the system temporary
directory. This measurement preceded the lower-canyon #506 deployment.
