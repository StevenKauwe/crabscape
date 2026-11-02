# Cellular archive capability: co-op design investigation

This is a proposed design, not an implementation or a replacement for the
current 64-page fail-closed guard. `World`, `shift_window`, and normal cellular
co-op have not been changed by this document.

## The boundary

An archive proof must cover the exact relation among the active window,
sleeping cells, pebble IDs and coordinates. A proof over one immutable borrow
cannot certify a later mutable `World`, a different destination with the same
origin, or a world after a streamed page/pebble mutation. The incremental
validator experiment safely holds an immutable borrow during its scan, but
its `Valid` result is not a transferable authorization.

`VersionedChunks` is an unwired generic copy-on-write experiment, not this
boundary: it neither owns a `World` nor its pebble table, and generic payloads
may have interior mutability. Comparing page versions alone cannot prove the
absence of an orphan owner in a newly inserted page, a missing footprint cell,
or a mismatch after an aggregate or the window origin changes.

## Candidate owner

Introduce a concrete private `CellularArchive` owner whose sole mutable state is
the `World`, fixed-size active arrays, sleeping-page map, pebble metadata and a
validated reverse owner index. No public mutable references to any of those
parts, and no generic or interior-mutable page payloads. Construct it only by a
bounded incremental validation of a frozen snapshot. Keep the session paused
until the scan succeeds; an interrupted, invalid, over-budget or stale scan
must not create an owner. The owner must be destroyed or revalidated on loading
an arbitrary checkpoint. Normal solo World and existing save paths retain their
current behavior until explicitly migrated.

Maintain these invariants for each stable pebble ID:

* every present archived owner cell is sand, outside the active window and
  within that ID's world-coordinate footprint;
* an entirely sleeping pebble has exactly `side * side` distinct archived
  cells; an active pebble has none; a partly clipped pebble is rejected;
* active owner cells match the active pebble footprint in both directions;
* archived page membership, pebble identities, window origin and owner counts
  are part of the same committed state. Pebble IDs never silently change when
  active pages sleep or wake.

Use world-coordinate page keys, not offsets relative to the moving window.
Per-page metadata can store a version plus per-owner counts and validated
footprints; a per-ID total makes the completeness check local to the changed
IDs. Page content, page membership, aggregate metadata and totals must be
private to the owner. Checked monotonic generations must fail closed on wrap;
new and removed pages invalidate the directory generation. A caller-provided
version number, hash, stale handle or same-origin check is not a capability.

## Bounded transaction

A mutation takes a bounded list of touched pages/active regions and affected
pebble IDs. Validate all inputs, reserve a bounded undo journal, and stage the
new cells and metadata. Recheck both old and new owner relations, including
page deletion, newly seeded pebbles, removed owners and all crossing edges.
Only commit page versions, directory membership, pebble table, origin, counts,
flow/material cells and wake/sleep state together; on any late failure restore
all state and leave the old proof valid. Reject excess pages, cells, IDs or
journal bytes before mutating. A page version can preserve its prior proof only
if both page bytes and every pebble/window relation it depends on remain
unchanged. Active-only movement still needs to update active ownership and the
corresponding global aggregate state.

`shift_window` currently scans and copies the active region, saves outgoing
cells individually, restores incoming cells, updates the origin, translates
all pebble coordinates and can seed aggregates. It cannot safely be wrapped
by incrementing a single counter after the call: it may mutate before failing,
may touch many pages, and would invalidate a proof retained by a detached copy.
A real integration needs staged per-page work plus an atomic publish barrier.
Until it is ready, pause/reject shifts outside the supported bounded envelope;
do not expose partially shifted terrain to either peer. Mutating many pages
may take multiple frames while a pinned version remains unchanged. The
underlying World must not advance concurrently with that staged operation.

A successful bounded update does not validate actors, RNG, clocks, wakes,
diffusion, kelp, transport authority or whole-session equivalence. Keep the
existing full terrain digest and gameplay guard during this work. Full-world
snapshots and `World` clones also remain a memory/performance blocker.

## Required proof before integration

* Demonstrate immutable ownership and reject direct archive mutation, stale
  proofs, cross-world proof reuse and counter wrap.
* Compare incremental initialization to exhaustive validation for malformed
  owner IDs, missing/duplicate/orphan cells, active/archive overlap, clipped
  aggregates, negative/extreme coordinates and arbitrarily ordered pages.
* Test page insert/remove/replacement, aggregate move/sleep/wake and window
  shifts across positive and negative seams. Compare each accepted state with
  the existing contiguous simulator, including flowing water, sediment,
  pebbles and aggregate motion; check conservation and exact reactivation.
* Inject failure after every staged page/owner change, verifying the complete
  world, index, versions, actors and outgoing messages are unchanged.
* Use actual host and guest Sims and source-bound transport; reject dropped,
  reordered, stale, forged and oversized operations without advancing either
  side. Test disconnect and original solo-world restoration.
* Measure worst-case work and memory per update, including page index, undo
  journal, full-digest/clone costs and snapshot lifetime, in browsers and on
  phones. Sparse synthetic microbenchmarks are insufficient for live play.
