# Trusted browser frontend in the existing introducer

This isolated Rust library embeds exactly thirteen public packaged assets. `mounted_router()` serves them at `/crabbridge/` on the existing `crabscape-introducer` Fly app. `/crabbridge` redirects to that fixed local path; unknown asset paths return 404, not HTML. GET/HEAD only. Every static response, including errors, carries the exact production policy extracted from `deployment.js`: no-store, frame denial, no-referrer and no permissive CORS. Third-party terminal notices remain included.

It contains **no** host gateway, CLI, credentials, filesystem routes, uploads, login API, new signaling server or analytics. Existing room/report handlers retain their CORS and body limits. The application's static route mount is separate from those permissive game routes. The public introducer origin becomes trusted for this UI; compromise of that app/origin can compromise terminal users. No unrelated service worker may control the page. Merely serving this page does not enable host passkey config or broaden native host ICE/network policy.

## Local checks

From repository root:

```sh
tools/build-crabbridge.sh
cargo test --locked --manifest-path tools/crabbridge/frontend/Cargo.toml
cargo test --locked --features crabbridge-frontend --manifest-path tools/introducer/Cargo.toml
cargo clippy --locked --manifest-path tools/crabbridge/frontend/Cargo.toml --all-targets -- -D warnings
cargo clippy --locked --features crabbridge-frontend --manifest-path tools/introducer/Cargo.toml --all-targets -- -D warnings
bash tools/test-introducer-context.sh
```

The optional standalone native binary defaults to `127.0.0.1:8080`. It serves production policy, not a relaxed HTTP development mode. Router tests start no listeners; the browser protocol tests use intercepted reserved HTTPS. `CRABBRIDGE_ASSET_DIR` is an optional trusted **build-time** input. Runtime cannot change assets or CSP. Rebuild packaged output from the exact reviewed revision before compiling, and verify `/crabbridge/build-commit.txt` after deployment.

## Curated container context

```sh
tools/build-crabbridge.sh
# Choose a fresh nonexistent directory; script refuses to overwrite anything.
bash tools/build-introducer-context.sh /tmp/crabbridge-introducer-context
```

The staging script copies explicit source/public-asset allowlists only, refusing symlinks. No gateway source/binary, tests, config, invitations, SDP, SQLite, screenshots, git history or local artifacts enter the context. Docker uses a non-root runtime. The existing introducer Dockerfile now requires this staged context, not `tools/introducer` alone.

The existing `.github/workflows/introducer.yml` deploys the app on qualifying merges to main. Integration code changes therefore become a real deployment when merged; this local implementation/test work does not deploy or authorize a merge. No new Fly app is needed. The parent workflow owns build/package/stage/deploy sequencing.

Hosting alone is not proof of off-Wi-Fi connectivity. Before claiming remote usability, verify real HTTPS headers, exact build identity, default-disabled controls on policy failure, no service worker, independently approved host ICE/STUN/TURN scope, and an actual external-device connection. Passkeys additionally require explicit host feature/config activation with exact HTTPS origin/RP. No activation is performed here.

The introducer defaults to game-only compilation with no embedded frontend dependency. Production Docker explicitly enables `--features crabbridge-frontend`; bridge CI must test that feature. Existing game-only browser fixtures remain independent of packaged bridge assets.
