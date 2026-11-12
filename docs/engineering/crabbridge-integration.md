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
- Bridge assets are a separate CI artifact. Publishing/preview assembly must explicitly merge that
  artifact; this change does not deploy a public host service or modify existing publishing behavior.

## Remaining publication boundary

Static pages do not create a host connection or grant any tools. A trusted frontend and explicit
signaling/ICE policy are still required. The verified LAN test does not prove arbitrary Internet NAT
traversal. Existing game service workers and same-origin pages are part of the trusted-code boundary
when hosting under the game origin; review that before public use with real host credentials.

The generated static directory does not inherit the local host server's CSP, frame restrictions,
cache policy or other security headers. Equivalent reviewed hosting policy must be configured before
public use; successful local asset loading is not proof that the production origin enforces it.
