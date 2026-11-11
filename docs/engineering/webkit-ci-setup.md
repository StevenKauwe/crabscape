# WebKit CI package setup

Phone tests, frame strips and the live-site smoke test share
`tools/install-webkit-deps.sh`. Each setup step has a five-minute budget, without
extending job budgets or changing gameplay checks.

The Ubuntu 26.04 image used by the November 11 failures already supplies a mirror
list: Azure first, then the Ubuntu archive and security archive. Our old source
rewrite replaced Azure with a duplicate archive endpoint, removing that diversity.
Preserve runner-managed sources, signing keys and verification instead.

The helper's command-scoped acquire settings match that image: one acquisition
retry and 15-second HTTP/HTTPS timeouts. These are package-download settings, not
test retries. They do not fully bound DNS resolution or all downloads together;
the step deadline bounds setup. `APT::Update::Error-Mode=any` prevents a failed
index refresh from silently continuing with stale metadata. Select the modern or
legacy driver from an actual installation candidate, then install exactly once.
A network or signature failure must not trigger a second install under another
package name.

Exact runner-image references:
- [Mirror configuration](https://github.com/actions/runner-images/blob/ubuntu26/20261101.186/images/ubuntu/scripts/build/configure-apt-sources.sh)
- [Acquire settings](https://github.com/actions/runner-images/blob/ubuntu26/20261101.186/images/ubuntu/scripts/build/configure-apt.sh)
- [Upstream stall report](https://github.com/actions/runner-images/issues/14594)

This restores mirror diversity and fixes failure masking and budget exhaustion.
It cannot guarantee DNS uptime. A broad outage will fail setup clearly, leaving
required browser checks failed rather than skipped or falsely successful.

Run `bash tools/test-install-webkit-deps.sh` for deterministic, network-free command
stub tests. These verify control flow, exact apt options, driver selection, error
propagation, and workflow wiring. They do not prove Linux package availability or
browser execution; the normal exact-commit Ubuntu CI run provides that validation.
