# Two-device CrabBridge browser proof

Use only for an authorized temporary LAN test. No installation, firewall changes, STUN/TURN or public hosting.

Prerequisites: existing Playwright and Chromium. Use PLAYWRIGHT_MODULE and CHROME_BIN environment overrides for the installed module and executable if needed. The isolated browser uses only the temporary WebRtcHideLocalIpsWithMdns feature flag; it does not disable Chromium sandboxing or grant media permissions.

Run: `HARBOR_FIXTURE_DIR=/private/test-dir HARBOR_CLIENT_IP=<authorized-client-ip> HARBOR_HOST_IP=<authorized-host-ip> node lan-browser.cjs /verified/frontend-assets`

The private fixture directory must contain mode 600 `invite.json` and `password.txt`. Assets must be separately hash-verified and contain remote.html, remote.js, matchbox.js, style.css, vendor/xterm.js, vendor/xterm.css, remote/pkg/harbor_noise.js and remote/pkg/harbor_noise_bg.wasm. Never put credentials in the asset directory.

The harness binds frontend HTTP only to 127.0.0.1:7878. It produces private offer.json containing only actually gathered numeric UDP host candidates matching the authorized client IP. Transfer the offer to the host through the private authorized channel, then write the returned answer to answer.json. The harness waits up to 4 minutes, checks Noise/password login and registered cat output, verifies the selected LAN candidate pair, then Stop/logs out. It checks the actual xterm buffer, not DOM text, and prints answer-application time. Permission queries are read-only and never grant local-network access. It always closes browser/server and removes the four private fixture files. `result.json` contains pass/fail and sanitized diagnostics only, never SDP or passwords.

A clean native host exit alone is not proof of successful connectivity. Check pass:true and selected_pair evidence. Failure diagnostics preserve only connection states, candidate metadata and packet counters. This harness is a test fixture, not a deployment.

## Host fixture

The test host must already be authorized for this exact LAN scope. Build with `RUSTC_WRAPPER= cargo build --locked --offline`. In a disposable private directory, run `codex-harbor init CONFIG`, register `/bin/cat` as tool `cat` with that directory as its workspace, and create user `lan-pilot` with `control` and `cat` grants using `add-user ... --password-stdin`. Generate a fresh invitation with `remote-invite CONFIG lan-pilot INVITE`. Transfer only that disposable invitation and password through a trusted private channel to the client's private fixture directory. Do not place them in frontend assets or commit them.

After receiving the offer, run `HARBOR_LAN_DIAGNOSTICS=1 codex-harbor remote-answer-lan CONFIG INVITE OFFER ANSWER` and deliver ANSWER promptly. Never reuse a consumed grant for another attempt. The host pilot currently supports only the explicitly reviewed 192.168.1.147 host and 192.168.1.198 peer pair; client environment variables do not change that host policy. Verify the native process exits and its exact UDP socket is gone, revoke/remove the disposable user and delete fixture secrets afterward. Preserve only sanitized pass/fail evidence. This manual physical-device check is not automatically run by `run-proof.sh`.
