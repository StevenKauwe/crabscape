# CI progress and publishing

## Active policy

CI uses one concurrency group per Git ref, `ci-${{ github.ref }}`, with
`cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}`. GitHub's default single pending slot is intentional:

| Event | Running work | Pending work |
| --- | --- | --- |
| Another push to main | Finishes, including deployment, live smoke and health reporting | Replaced by the latest run arriving in this group |
| Another update to the same PR | Superseded run is cancelled | Latest update remains |
| Update to another PR | Independent group (`refs/pull/<number>/merge`) | Independent pending slot |
| Manual CI on main | Same policy and group as pushes to main | Coalesces with main pushes |
| Manual CI on another branch | Superseded run on that branch is cancelled | Latest run remains |

For example: main A is running; B arrives and waits; C arrives and replaces pending B. A can validate and deploy
its own tested artifact, then C can run. Continuous pushes cannot cancel A before it finishes. This is not a FIFO
backlog of all intermediate commits. GitHub does not guarantee dispatch ordering: "latest" here means the latest
run entering the pending slot, not an ancestry guarantee for reruns or delayed events.

The workflow-level group covers the whole run, not only validation. Its required jobs, job conditions, permissions,
and deployment gates are unchanged: main deployment needs `check`, `determinism`, `art`, `web`, and `browser`;
the browser gate includes the browser tools, WebKit matrix and co-op matrix. Deployment downloads
`crabscape-web-${{ github.sha }}` from the same run that tested it. It does not rebuild, select "latest green", or
promote a PR's synthetic merge artifact. There is no requirement that this tested SHA still be the current main tip.

The policy follows [GitHub's concurrency semantics](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency):
one running and one pending by default, pending replacement, and conditional cancellation. Do not set `queue: max`
on validation: processing every main revision would restore a growing backlog.

## Shared publisher follow-up (not implemented)

Main's deploy job currently has a `pages` lock; PR/branch preview publication and `previews.yml` cleanup do not share
it. The store's force-with-lease retries protect store updates, but snapshots can still be deployed out of order.
The live checks also do not assert the exact deployed commit. The cancellation change does not solve these races,
late reruns regressing production, or the trust boundary of the current PR preview job.

Adding the same default concurrency group to all three writers is insufficient: a pending cleanup for PR 1 could
be replaced by a preview for PR 2. Moving publication to `workflow_run` also grants a new privilege boundary and must
not execute scripts or downloaded tools from an untrusted triggering revision.

Implement the following together in a separate publisher change:

1. **Separate validation from publication.** CI has read permissions and uploads immutable web bytes plus a build
   manifest. A protected default-branch workflow uses trusted publisher code, a restricted environment and minimal
   job permissions, with a verified policy binding the allowed publisher identity. Remove the old main, preview
   and cleanup publishing paths in the same cutover, leaving one writer.
   Preserve required check names and dependencies. PR validation must never gain publication credentials.
2. **Bind requests to verified source runs.** For a completed CI event, query the triggering run by ID and verify
   repository ID, workflow ID/path, event, branch, conclusion, run attempt and source commit. Download the artifact
   by that run and artifact ID; validate its digest and manifest against API metadata. For main accept only a green
   main push or explicitly supported main manual run, with every required gate successful. Record the source SHA,
   run ID, attempt and artifact identity. For PRs distinguish the PR head SHA from the checked-out synthetic merge
   SHA and bind both to the actual source run and current open PR. PR bytes may only enter that PR's folder. Do not
   infer the triggering build from the publisher's `github.sha`, and never promote synthetic PR artifacts to main.
3. **Keep durable desired state for every destination.** Use a protected append-only request journal, keyed by
   main/PR/branch destination and source identity, including a tombstone for a closed PR. Authenticate requests from
   GitHub metadata, not arbitrary artifact fields. Record each request before requesting a publishing lock; journal
   writes use compare-and-swap retries and do not share a coalescing concurrency group. Duplicate events are
   idempotent. A later reconciliation pass must see every distinct destination even if its original wake-up was
   replaced, failed, or arrived during publication. Add a scheduled recovery sweep and a visible pending/error
   summary; retain requests until their committed state is verified live. GitHub's optional `queue: max` is bounded
   (100 pending), so by itself it is not a lossless journal.
4. **Reconcile inside one publication lock.** Once holding `pages`, read the journal and latest store, validate live
   PR state again, and apply all outstanding destinations. A close tombstone wins over an older preview completion;
   reopen requires a newly validated build. For main, accept a candidate only if it equals or descends from the
   recorded production SHA; reject an older or divergent rerun. A newer main tip does not disqualify a tested
   intermediate commit. Distinct previews survive each other's updates and cleanup only removes its own folder.
   Assemble the snapshot inside this lock, deploy that snapshot, and keep the lock through identity verification
   and acknowledgement. A request arriving after the snapshot stays pending for the next sweep.
5. **Treat artifacts as data.** Never run artifact scripts, binaries, build hooks or PR checkout code with write
   credentials. Validate file types, paths, symlinks and size before extraction into a staging directory. Use trusted
   code to build the index and validate identities. Run browser tests in a separate unprivileged job, never in the
   publisher with credentials; do not run downloaded PR browser-test tools in the privileged workflow. Preview
   pages execute in visitors' browsers: account for the existing same-origin Pages trust boundary in the design.
6. **Verify the bytes that went live.** Publish a trusted manifest for the snapshot and each build, binding the
   deployment to its commit, run, attempt and artifact digest. Fetch that manifest and `pkg/commit.txt` with cache
   busting and a bounded propagation wait before acknowledging requests. A marker proves the served metadata;
   also compare fetched web/Wasm bytes to the artifact digests where feasible to prove the served build itself.
   The protected publisher can read static
   identity files without executing website code. Keep functional browser smoke unprivileged and bind its expected
   identity to the publication receipt. If another publication replaces it before a live test, report it as
   superseded rather than claim the old run was verified. Carry that receipt into `coop.yml` instead of assuming
   any successful CI run describes the current live site. Attribute failures and health reporting to that receipt.

## Acceptance evidence

For the cancellation change, lint the workflow and confirm its change-selection flags still enable all heavy jobs.
Inspect the exact PR head's CI results; PR CI cannot prove main scheduling. After this change is merged through the
normal process, observe main A running while B and C arrive: A must finish, pending B must be replaced, and C must
run next if nothing newer arrives. Observe a second update to one PR cancelling its first run while another PR
continues independently. Do not create production pushes merely to exercise these cases.

Before replacing the publisher, add deterministic Rust or shell integration tests with a local store and a fake
GitHub/Pages interface. Cover A/B/C main arrivals, old-main reruns after a newer deploy, preview plus main plus
cleanup in either order, two different PR cleanups, close during a preview build, reopen, duplicate events, failed
deployment recovery, journal writes during snapshot assembly, and bursts beyond 100 requests. Reject wrong
repository/workflow/event/run/attempt/artifact identities, missing or unsuccessful required gates, archive traversal,
symlinks, and PR-to-main promotion. Make a delayed old snapshot unable to overwrite a newer published snapshot;
verify stale live manifests fail identity verification. Test browser smoke with no publication credentials.

Do not claim the shared publisher is complete until these tests and an isolated deployment exercise pass. The
one-line CI cancellation fix deliberately leaves that cutover for its own review and validation.
