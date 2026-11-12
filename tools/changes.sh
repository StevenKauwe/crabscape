#!/usr/bin/env bash
# Component-aware CI selection. Unknown/empty input runs every component.
# Docs alone skip heavy jobs. Never use workflow path filters for required checks.
set -euo pipefail
any=false docs_only=true code=false art=false introducer=false bridge=false
while IFS= read -r path; do
  [ -n "$path" ] || continue
  any=true
  case "$path" in
    *.md | docs/* | .claude/skills/*) continue ;;
  esac
  docs_only=false
  case "$path" in
    tools/crabbridge/frontend/* | tools/crabbridge/web/* | tools/crabbridge/transport/auth/* | tools/crabbridge/transport/Cargo.toml | tools/crabbridge/transport/Cargo.lock | tools/crabbridge/transport/build.sh | web/crabbridge/* | tools/build-crabbridge.sh | tools/build-introducer-context.sh | tools/test-introducer-context.sh)
      introducer=true; bridge=true; continue ;;
    tools/crabbridge/* | web/crabbridge/* | tools/build-crabbridge.sh | tools/test-build-crabbridge.sh | tools/test-crabbridge-page.cjs)
      bridge=true; continue ;;
    tools/introducer/* | .github/workflows/introducer.yml)
      introducer=true; bridge=true; continue ;;
    # Shared build/security/publishing controls affect both products.
    tools/changes.sh | tools/test-changes.sh | tools/check-component-gate.sh | tools/test-component-gate.sh | tools/ci-changes.sh | tools/test-ci-changes.sh | tools/previews.sh | tools/test-previews.sh | tools/compose-web.sh | tools/test-compose-web.sh | tools/test-bridge-publish-policy.sh | \
      tools/main-health.sh | tools/wait-pages-artifact.sh | tools/test-pages-artifact-wait.sh | \
      .github/workflows/ci.yml | .github/workflows/previews.yml | .cargo/config.toml | \
      rust-toolchain.toml | Cargo.toml | Cargo.lock | deny.toml | .gitignore)
      code=true; art=true; introducer=true; bridge=true; continue ;;
  esac
  code=true
  case "$path" in
    src/* | crates/dragon-peer/* | crates/king-ddd/* | tools/kingworker/* | tools/build-king.sh | assets/king-ddd/* | \
      crates/sim/* | crates/listen/* | web/* | tools/webtest/* | tools/cooptest/* | tools/tidy/* | tools/cellworker/* | vendor/* | \
      tools/build-web.sh | tools/build-dragon.sh | tools/determinism.sh | tools/mutants-pr.sh | \
      .cargo/mutants.toml | .github/workflows/mutants-weekly.yml | .github/workflows/coop.yml | .github/workflows/coverage.yml) ;;
    crates/crabrig/* | crates/painter/* | tools/oil-atlas/* | tools/paint-oil.sh | art/* | assets/*) art=true ;;
    *) art=true; introducer=true; bridge=true ;;
  esac
done
if [ "$any" = false ]; then
  docs_only=false code=true art=true introducer=true bridge=true
fi
printf 'docs_only=%s\ncode=%s\nart=%s\nintroducer=%s\nbridge=%s\n' "$docs_only" "$code" "$art" "$introducer" "$bridge"
