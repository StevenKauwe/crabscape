# Opening pebble migration: design only

Fresh standard worlds no longer contain the baked starting pebble bed. Existing
cellular saves contain the actual evolving terrain, so changing the opening bake
cannot update them. No migration is currently installed.

## Eligibility and scope

The v1/v2 cellular envelopes contain no reliable flag saying whether their world
came from the older bake. Owner IDs are allocated in insertion order, but new
worlds also start allocating from ID 1, and saved aggregates can move. A prefix
of IDs, the terrain version, and the shape/shade of a pebble are not proof of
its origin. Do not infer provenance or silently remove a supposedly historical
set by ID.

If the player elects a one-time cleanup for an existing world, define its scope
in world coordinates: select only whole pebble aggregates whose complete current
footprint is within an explicitly specified opening rectangle. Skip anything
crossing its boundary. This is a location-based cleanup, not an assertion that
the chosen pebbles came from the original bake. The exact rectangle is a product
choice and must be displayed or otherwise made clear before applying it. Ordinary
sand, mud, rock, water, the other pebbles, the crab and progress remain as they
are. A pebble outside the rectangle that was carried there historically is not
selected; a later pebble currently inside it would be selected. Empty selection
is a successful no-op, not a license to run the cleanup again later.

Validate the full checkpoint and every aggregate's active and sleeping owner
coverage before changing anything. Construct a separate candidate. Clear the
selected owners to ordinary sand in both active and sleeping cells, retaining
cell position and fluid state; assign selected cells deterministic ordinary-sand
grain values so they no longer render or behave like the heavier pebble grain.
Remove selected aggregate entries and their matching motion entries, and remap
all surviving owner IDs in every active and sleeping cell. The current runtime
uses index-plus-one owner IDs; simply removing vector entries invalidates later
IDs. Verify both directions of the owner/footprint mapping, the retained pebble
motion, unaffected terrain, saved position and player progress before and after.
Any unsupported version, malformed mapping, excessive size, or failed check
leaves the original record intact and autosave disabled for that attempted load.
Do not change the original snapshot or install a partial candidate while this
work is pending.

## One-time identity, backup and commit

The migration needs a durable per-world identity and a decision marker, not a
process-local flag or a location check rerun on every load. A marker must record
at least the migration version, source identity/digest and the resulting save
revision; it must distinguish successful no-op, completed migration, reset and
import/new-world cases. A new or imported world receives its own identity and an
explicit migration decision; never copy a prior world's marker blindly. A reset
must fence outstanding migrations and remove or supersede their markers together
with their saves. Retaining only a hash of mutable save bytes as identity is
insufficient: every ordinary autosave would look like a new world. There is no
current cellular character import; any future import must establish these rules
before it is enabled.

Browser design: under one IndexedDB read-write transaction, compare the captured
source bytes/revision and existing reset epoch, preserve an immutable pre-migration
backup, and publish the fully encoded migrated save plus its marker. No new head
or marker may become visible without the other. Discard worker results when the
epoch, source or decision changed. Verify the completed envelope and enforce the
existing packed/raw limits before the transaction; on quota, abort, missing
backup, malformed data or transaction error preserve the exact prior source.
Keep the backup through later autosaves so recovery remains possible; define
bounded retention before implementing it. A preexisting tab or older build that
does not respect the new epoch/lock/marker can still write legacy data: cross-
version coexistence is a release blocker, not something the new marker alone fixes.

Desktop design: a separate marker sidecar cannot be atomically updated with the
current save via rename. Use a single self-contained, versioned envelope carrying
both terrain and migration decision, with an independently retained verified
backup and crash-safe file replacement, or another protocol with equivalent
atomicity. Prevent stale background writes and concurrent processes from replacing
it. Define reset and import under the same durable generation. Do not create a
marker first and hope a later write succeeds. An old build may not understand a
new envelope and can overwrite its file; backwards and cross-process behavior
must be resolved before enabling the change.

The v2 replay token describes the original terrain. It must never be reused for
the transformed world. Generate and validate a new replay checkpoint from the
candidate before installation, using bounded asynchronous preparation on browser;
if it cannot be made available, do not represent the old token as valid. Existing
replay files remain immutable and continue to replay their original worlds; this
migration only changes the new session's initial checkpoint. Verify old and new
replay determinism separately, including subsequent autosaves and reloads.

## Required validation before live wiring

- Legacy and new saves, missing/invalid metadata, active and sleeping aggregates,
  edge-crossing pebbles, moved pebbles, multiple owner remaps, and worlds without
  selected pebbles; assert exact unrelated cell, actor and progress preservation.
- Replay from the migrated checkpoint and old replay files; prove tick, player,
  terrain and future hashes agree with their respective references.
- Browser actual transaction abort/quota/error, interruption, concurrent reset,
  pending worker results, and supported cross-tab/cross-version writers; restore
  the exact pre-migration bytes and ensure no silent marker-only success.
- Desktop interruption at each durable-write boundary, worker/reset races,
  backups, concurrent processes, and atomic marker/save recovery.
- Fresh-world, import and reset behavior so the decision is once per world, not
  once per installation or accidentally repeated after an ordinary autosave.
- Memory, startup latency and worker availability on actual target devices;
  source backup plus candidate and replay preparation can temporarily hold
  multiple copies of a large world.

This proposal is intentionally not a migration or permission to rewrite saves.

## Automatic publication state machine (proposal)

Automatic handling cannot identify historical origin from v1/v2 bytes. It would
therefore apply the chosen coordinate-scoped rule once to every eligible
unmarked legacy world, including any newer world saved before markers existed.
That behavior and its fixed rectangle must be accepted before activation. The
marker must be written for newly created worlds from their first save, so they
are not later misclassified as legacy. A source whose provenance is ambiguous
or whose marker disagrees with its bytes must remain untouched; do not treat a
missing or damaged marker accompanying known newer metadata as legacy.

A browser implementation can use these states, all scoped to one build-specific
IndexedDB database and a persisted reset epoch:

1. **Read:** In one read-only transaction capture the epoch, complete active
   save bytes and migration metadata. For a marked world, require the marker's
   current-record digest to match the captured bytes. A marker/record mismatch
   means an external, older-client or partial write; freeze autosave and surface
   recovery instead of migrating or falling back. For an unmarked source, capture
   exact bytes plus the epoch as the compare-and-swap input. Corrupt, missing or
   unrepresentable data fails closed.
2. **Prepare:** Off the main thread where practical, validate the source and
   transform a separate complete candidate. Validate every owner and player,
   encode and independently verify the new save, and create a replay checkpoint
   of the transformed terrain. Bound allocations and do not mutate the running
   game or overwrite the source. If selection is empty, prepare a no-op decision
   tied to the exact source. A failed candidate or unsupported replay stays at
   the original source; it is not a completed migration.
3. **Commit:** In one read-write transaction, reread and compare the epoch,
   source bytes and decision state to the captured values. If they differ, abort.
   Write a verified immutable backup of the pre-migration record, the new active
   record and a marker with a new persistent world ID, migration decision and
   current-record digest; only transaction completion establishes success. For a
   no-op, atomically preserve the source and write the decision/identity. If a
   backup already exists, verify its identity and retention policy rather than
   replacing it. Quota, abort or failed validation must not publish any subset.
4. **Install:** Before ticking or enabling autosave, install the fully validated
   candidate and its matching replay checkpoint. If installation is canceled,
   times out, throws or the outcome of the storage transaction is unknown, do
   not infer rollback and do not start a fresh writable session: remain paused,
   reread durable state under the epoch fence, and recover or retry. A confirmed
   commit can be loaded on the next startup; a confirmed failed transaction can
   retain the old source. Avoid attempting a blind compensating overwrite after
   another tab has advanced or reset the world.
5. **Subsequent saves:** Every current-client autosave atomically compares the
   world ID, epoch and expected active revision, then replaces active bytes and
   updates the marker's current-record digest in the same transaction. This
   detects an old client or external import writing the active record without a
   corresponding marker, rather than silently treating that write as a new
   legacy save. A session cannot refresh its expected epoch after reset and
   continue writing old state.
6. **Reset or import:** Reset advances the epoch and atomically removes the
   active record and its current decision; its exact backup-retention policy is
   explicit and cannot be confused with an active save. Import atomically creates
   a new world identity and matching initial decision/digest in the same epoch
   transaction, after validating the imported source. In-flight pre-reset or
   pre-import stages, worker results and install tickets must fail their compare.

The digest is an integrity and identity check, not a substitute for an atomic
transaction or an authorization mechanism. Hashing or comparing a whole record
inside an IndexedDB transaction can be expensive or deactivate the transaction;
the exact implementation must use transaction-safe operations and verified
precomputed digests bound to the source. A modern-tab Web Lock can reduce
contention but cannot fence an older client that ignores it. A legacy writer can
overwrite active bytes before the marker mismatch is detected; without a durable
cross-version exclusion mechanism the protocol can detect that condition and
preserve the backup, but cannot guarantee the newest progress survives. This is
still a live-release blocker.

## Isolated browser experiment

A `cleanup-proof` feature, enabled only by the separate `cellstore-proof` Wasm
crate, exposes the reviewed pure transform and historical fixture. The normal
game dependency does not enable this feature. A focused Chrome proof runs with
`CRAB_PROOF_CLEANUP=1` on the existing Rust browser runner. It constructs v1
and v2 records from the actual old bake, transforms the candidate, rejects the
old terrain replay token, validates/reconstructs the candidate and saved position
before publication, regenerates a replay, and checks the committed save against
replay playback. A proof-only IndexedDB transaction tests source/epoch
comparison, backup + candidate + marker + replay publication, injected abort,
reset/import fences and detection of an external overwrite.

This is not production publication: it runs in a dedicated proof database, uses
synchronous full-record work and retains multiple copies, and does not yet
prove a document reload, real quota exhaustion, phone memory/latency, older-client
exclusion, or recovery from every late asynchronous transaction. Its record
format and import/reset semantics are test scaffolding, not a migration marker
for actual game saves. The low-level transaction accepts arbitrary candidate
bytes and does not itself enforce those fixture validation steps. Complete loss
of marker, backup and replay keys is indistinguishable from an unmarked legacy
record. The existing broader v1/v2 reload proof is separate.
