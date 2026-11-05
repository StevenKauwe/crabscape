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
