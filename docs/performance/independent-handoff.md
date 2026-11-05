# Keep the host moving during an independent-region transfer

After a shared region splits, the owner installs two disjoint leases, then sends
the guest's checkpoint. Previously the host also stopped ticking throughout that
transfer, despite having no replica for its own region. The host can now tick its
independent lease while `Sending`; shared-region handshakes still stop both sides.

The host waits if another topology change, lease edge or Home request needs to
be processed. Pending actions and regrant requests are retained. Sediment
arrivals addressed to the guest remain in the owner ledger until Ready, so they
cannot target the guest's old kernel or be lost during checkpoint installation.
The existing Ready check validates the guest's journal tick independently of the
host's current tick. Local journal batches continue to record every host tick.

This does not remove stop acknowledgement, collection, checkpoint creation or
the guest's own transfer pause. It does not change the simulation, terrain
representation, save format or Claude's unfinished CPU/remapping spike.

## Real-browser regression

```sh
CRABSCAPE_PROFILE_GPU=1 \
CRAB_HANDOFF_WEB=/absolute/path/to/web \
CRAB_HANDOFF_INTRODUCER=/absolute/path/to/introducer \
cargo test -p cooptest --test handoff_progress -- --ignored --nocapture
```

The existing `cells-session` travel fixture moves the guest into its own region,
records browser frame gaps and simulation stalls, then verifies both regional
replays against native simulation. The regression additionally requires host
ticks to advance between Sending and Running for the same region/revision.
A rendered transfer with frozen ticks fails; this avoids a machine-dependent
millisecond gate while enforcing the behavior that removes the needless wait.
The test is opt-in because it needs a compiled web game, introducer and Chrome.
For full frame intervals and an optional calibrated gap budget, use `frame_probe`
from PR #500; rendering FPS and simulation progress are separate measurements.

## Measured validation

On the same Mac with hardware-rendered Chrome, using CI's Binaryen 133 `-Oz`
optimization for both builds, the deployed `6dbbbfc` negative control failed
this regression: host tick 1320 stayed unchanged throughout Sending and Running,
with a 300.1 ms simulation stall. Ordinary traversal and both native replay
comparisons passed, isolating the assertion to host progress.

The candidate passed: the host advanced from tick 1275 to 1295 during transfer,
and its measured simulation stall was 133.3 ms, ending while still Sending.
The guest's stall was 166.6 ms in both runs. Neither run recorded a rendered-frame
gap above the existing profiler's 60 ms reporting threshold. This is one paired
local measurement, not a phone benchmark or a universal latency guarantee.

The separate `cells-regions` browser scenario also passed: both players dug,
reunited through Home with identical world hashes, reproduced both regional
replays natively, and restored the private world after owner loss. Its stage-based
handoff duration remained about 319 ms: that measures the entire protocol, while
the host now resumes simulation before the protocol finishes. The guest pause,
stop acknowledgement and collection costs remain. The unrelated WebGL program
setup hitch is not fixed by this change.

## Read the diagnostic quantities separately

The opt-in regression prints reports for **both host and guest**:

- `completed_handoff_state_intervals`: observed Running → non-Running → Running
  intervals. These include protocol work during which an independent host can
  still advance its simulation; they are not automatically freezes.
- `recorded_unchanged_tick_stalls_over_75ms`: intervals in which the sampled
  simulation tick did not change, reported once ticks resume.
- `recorded_frame_gaps_over_60ms`: requestAnimationFrame intervals above the
  existing reporting threshold. Rendering and simulation can stall independently.
- `split_work_spans`: individual `cell-split-collect`, `cell-split-grant` and
  `cell-split-install` observations, grouped by name with original timestamps and
  durations. They cover measured work, not the entire asynchronous protocol.

Empty stall/gap lists mean nothing exceeded those reporting thresholds; they do
not prove zero latency. Unfinished handoffs and captures starting outside Running
are marked explicitly. The observer samples states once per rendered frame, so
it can miss transitions within a frame. Tick stalls still in progress when the
capture ends are not emitted by the existing profiler. Keep the raw traces when
investigating a stall. No new hard timing gate is imposed before runner calibration.

To inspect a deployed build instead of a local web directory:

```sh
CRABSCAPE_PROFILE_GPU=1 CRAB_HANDOFF_URL=https://crabscape.com/ \
cargo test -p cooptest --test handoff_progress -- --ignored --nocapture
```

Use a native checkout matching the deployed simulation so the replay comparisons
remain meaningful. URL mode uses the deployed introducer and the test's disposable
browser profiles; it does not read or overwrite a player's ordinary browser save.
Unset `CRAB_HANDOFF_URL` to use the local web/introducer variables above. Keep heavy
builds off the machine during performance captures and record hardware, renderer,
build and route when comparing results.

Synthetic tests deliberately separate a 600 ms protocol interval, an 80 ms tick
stall and a 100 ms frame gap, verify the work-span grouping, and reject malformed
timing data. The frozen-host negative control remains unchanged. These diagnostics
are measurement and regression coverage, not a fix for the remaining stutter.

The shared Chrome helper also observes `data-cell-frame-error`. Its timing line
remains in the existing short co-op log, while `cooptest-frame-error` in the tab's
sessionStorage retains the full latest diagnostic and timestamp across reloads.
`coop_log()` includes that untruncated record in ordinary failure reports. This
preserves the game's existing InvalidFrame details without changing simulation or
acceptance criteria. Reports print any available frame traces before failing the
existing traversal/replay assertion.
