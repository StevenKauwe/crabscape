# Hosting policy: explicit activation, not automatic trust

The published asset bundle starts locked. Before invitation/password handlers are installed, it checks a same-document HEAD response for the exact required policy. It also rejects framing and a controlling service worker, and clears the connection if a worker takes control later. These are conservative configuration/accident checks, **not** proof of the navigation response headers, publisher integrity or same-origin isolation. Compromised same-origin code can replace or use the client. No root service worker was found in the inspected game source; stale or unrelated registrations remain possible.

Required production response headers for both `/crabbridge/` documents (`index.html` and `remote.html`):

```
Content-Security-Policy: default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' wss://crabscape-introducer.fly.dev; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
X-Frame-Options: DENY
Referrer-Policy: no-referrer
X-Content-Type-Options: nosniff
Permissions-Policy: camera=(), microphone=(), geolocation=()
X-CrabBridge-Policy: 1
Cache-Control: no-store
```

Use the same restrictive policy for static JS/WASM responses where applicable and correct MIME types. No CDN scripts or injected analytics. Keep credential files and host configuration outside all served directories. HTTPS must be provided by a trusted, explicitly selected publisher. A path beneath the game shares that origin's trust; it does not isolate terminal credentials from other same-origin applications. Persistent device credentials remain a separate reviewed decision.

GitHub Pages does not provide these custom response-header controls directly. Do not claim an `_headers` file or a meta tag supplies them. On unconfigured Pages, CrabBridge remains locked. A header-capable trusted hosting/proxy arrangement or a deliberate different origin requires an explicit deployment decision; none is configured by this PR. The existing CrabScape Matchbox rendezvous remains unchanged regardless of frontend host choice.

Exact `http://127.0.0.1` is supported for development with the same headers but the development CSP additionally allows the fixed loopback broker and Matchbox fixture destinations. No localhost bypass skips header validation, and production never accepts those extra destinations.

Publication code separately defaults off unless repository Actions variable `CRABBRIDGE_PUBLISH_ENABLED` is exactly `true`. Setting it, merging, or running a deployment is not authorized by this source change. Enable only after the hosting/trust policy and deployment itself are approved. Remembered login is not yet implemented; never store a password, session token or reusable invitation as a shortcut.
