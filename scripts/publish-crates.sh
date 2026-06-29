#!/usr/bin/env bash
# Publish the open-subtitle library crates to crates.io in dependency order.
#
# These 10 library crates are published so that downstream consumers (e.g.
# open-media's `om-subs`) can depend on them by version from the registry — which
# is required for `cargo package` (release-plz change-detection) to resolve them.
# The 4 frontends (os-cli/os-daemon/os-ffi/os-mpv) are NOT published.
#
# Auth: requires a crates.io API token for the publishing account. Provide it via
#   `cargo login`  (writes ~/.cargo/credentials.toml), or
#   CARGO_REGISTRY_TOKEN=<token> in the environment.
#
# Idempotent-ish: a crate already at this version on crates.io will error
# ("already uploaded"); that's safe to ignore on a re-run.
#
# Usage:
#   ./scripts/publish-crates.sh            # publish for real
#   ./scripts/publish-crates.sh --dry-run  # verify only, no upload
set -euo pipefail

DRY=""
[ "${1:-}" = "--dry-run" ] && DRY="--dry-run"

# Dependency order: os-core first (everyone needs it), then the single-dep crates,
# then os-engine, then os-compose (depends on all of them) last. crates.io needs a
# tier indexed before the next tier that requires it, so we wait between tiers.
TIER1=(open-subtitle-core)
TIER2=(open-subtitle-config open-subtitle-identify open-subtitle-providers \
       open-subtitle-process open-subtitle-sync open-subtitle-translate \
       open-subtitle-transcribe open-subtitle-engine)
TIER3=(open-subtitle-compose)

publish_tier() {
  for crate in "$@"; do
    echo ">>> publishing $crate $DRY"
    cargo publish -p "$crate" $DRY
  done
}

publish_tier "${TIER1[@]}"
[ -z "$DRY" ] && { echo "waiting for crates.io to index tier 1..."; sleep 20; }

publish_tier "${TIER2[@]}"
[ -z "$DRY" ] && { echo "waiting for crates.io to index tier 2..."; sleep 30; }

publish_tier "${TIER3[@]}"

echo "Done. Published: ${TIER1[*]} ${TIER2[*]} ${TIER3[*]}"
