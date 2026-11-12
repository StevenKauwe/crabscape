# CrabBridge integration boundary

CrabBridge is a separately built Rust host tool, transport workspace and browser frontend at
`tools/crabbridge/`. It does not enter the game Cargo dependency graph or change the co-op introducer.
The browser frontend is packaged to `web/crabbridge/`, like other independent experiences, but no host
configuration, pairing material, screenshots, SDP, private fixtures or test results are publishable assets.

## Local checks

- `bash tools/test-changes.sh`: component selection and fail-closed regression checks.
- In `tools/crabbridge`: `bash tests/run-proof.sh` runs native authentication/authorization tests,
  transport tests, Rust/WASM build and real browser regressions. Existing pinned Playwright dependencies
  are isolated here, never added to the game pipeline. This is a scoped browser-test exception to the
  Rust-only tooling preference, not a new game runtime dependency.
- `bash tools/build-crabbridge.sh`: build the Rust/WASM crypto frontend and package an explicit asset
  allowlist. Requires the wasm-bindgen version in the bridge transport lockfile. Uses a fresh staging
  directory so stale private files cannot survive into the output. Does not deploy.

The host and transport retain their own Cargo locks and workspace boundaries. CI must not substitute
its game lockfile or silently install a different binding-generator version.

## CI contract

- Bridge-only changes run bridge checks, not game/art/replay checks.
- Game-only changes run existing game checks, not bridge checks.
- Shared infrastructure and introducer changes cover both relevant products.
- PRs classify their merge diff; main pushes classify the exact before/after commit range. Manual runs,
  new refs, missing commits, unknown or empty change lists select everything; classifier failures fail the protected gate.
- The existing protected `fmt, clippy (native + wasm32), tests` name is retained as an always-running
  gate over game checks and bridge checks. Branch protection is unchanged.
- Bridge assets are a separate CI artifact. Publishing/preview assembly explicitly composes that artifact with the preserved game component.
  This does not deploy a public host service or change the game introducer.

## Remaining publication boundary

Static pages do not create a host connection or grant any tools. A trusted frontend and explicit
signaling/ICE policy are still required. The verified LAN test does not prove arbitrary Internet NAT
traversal. Existing game service workers and same-origin pages are part of the trusted-code boundary
when hosting under the game origin; review that before public use with real host credentials.

The generated static directory does not inherit the local host server's CSP, frame restrictions,
cache policy or other security headers. Equivalent reviewed hosting policy must be configured before
public use; successful local asset loading is not proof that the production origin enforces it.

## Component artifact composition

Publishing jobs download selected artifacts into `artifacts/game` and `artifacts/bridge`, not into
checkout source files. `compose-web.sh` runs inside the existing gh-pages force-with-lease transaction:

- Verify all selected artifacts are regular trees with an index and the exact workflow SHA stamp.
- Preserve unselected components from the current store; bridge-only cannot create an empty game root.
- A new preview seeds the unselected component from the published main root, excluding all preview folders.
  `pkg/commit.txt` and `crabbridge/build-commit.txt` retain each component's actual source revision; a preview
  can intentionally contain a newer baseline component than its branch. `build.txt` identifies preview head.
- Reject main component rollback/divergence and stale preview heads. A force-pushed/divergent preview must
  be explicitly removed before republishing, rather than silently replacing an incomparable revision.
- Store lease conflicts restart composition against the latest store snapshot; they never reuse a stale
  composed tree. Existing shared publication concurrency still serializes the entire publish transaction.

`test-compose-web.sh` exercises preservation, stamp mismatch, missing baseline/artifacts, symlink rejection,
ancestry failures and a real local bare-git store roundtrip. `test-previews.sh` retains existing store tests.
These changes describe executable publishing code, not evidence of a deployment: no production publishing
command was executed during implementation. Host access remains separate from static frontend publication.

### Publication is disabled by default

New bridge publication is gated by the repository Actions variable
`CRABBRIDGE_PUBLISH_ENABLED` being exactly the string `true`. Missing, empty or `false` leaves
bridge artifacts private to CI: bridge-only changes cannot enter a publisher, and mixed/game builds
do not download or select the bridge artifact. Game publication still preserves any existing stored
bridge rather than deleting it. The normal protected checks continue to run regardless of this switch.

Only after explicit deployment approval should a repository administrator set that Actions variable
to `true` and initiate the intended checked workflow. This code change does not set the variable,
change repository permissions, or authorize a deployment. `test-bridge-publish-policy.sh` checks
job admission, artifact selection and artifact-download guards for both publishers.
