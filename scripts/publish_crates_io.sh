#!/bin/bash
# Publish DAF workspace crates to crates.io in dependency order, starting
# DELAY_SECONDS from invocation (default: 4 hours).
#
# Usage:
#   nohup bash scripts/publish_crates_io.sh > /tmp/crates-publish.log 2>&1 &
#   # or with a custom delay (seconds):
#   nohup bash scripts/publish_crates_io.sh 3600 > /tmp/crates-publish.log 2>&1 &
#
# Notes:
# - daf-core v0.1.0 is already published; it is skipped automatically if
#   the registry already has it.
# - Uses --allow-dirty so the pending Cargo.toml metadata fixes are included
#   without requiring a git commit.
# - Handles crates.io 429 rate limits by sleeping until the server-provided
#   retry time (+60s buffer) and retrying. Sleeps between crates so the
#   index can propagate dependencies.

set -u

DELAY_SECONDS="${1:-14400}"
WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SETTLE_SECONDS=120   # pause between successful publishes (index propagation + rate limits)
RETRY_BUFFER=60      # extra seconds added on top of server-provided retry time
MAX_ATTEMPTS=20

# Dependency order: leaves first. daf-core already live but kept in the
# list so the script is idempotent (skipped if already on the registry).
CRATES=(
  daf-core
  daf-runtime
  daf-repository
  daf-algorithms
  daf-cache
  daf-messaging
  daf-application
  daf-http
  daf-ffi
)

log() {
  echo "[$(date -u '+%Y-%m-%d %H:%M:%S UTC')] $*"
}

is_already_published() {
  # curl the crates.io API: 200 means this exact version exists
  local name="$1" version="$2"
  local code
  code="$(curl -s -A 'thedaf-publish-script' -o /dev/null -w '%{http_code}' "https://crates.io/api/v1/crates/${name}/${version}")"
  [[ "$code" == "200" ]]
}

crate_version() {
  # read version from the crate manifest (follows workspace inheritance)
  local name="$1"
  cargo metadata --no-deps --format-version 1 --manifest-path "${WORKSPACE_ROOT}/Cargo.toml" \
    | python3 -c "import json,sys; print(next(p['version'] for p in json.load(sys.stdin)['packages'] if p['name']=='$name'))"
}

publish_one() {
  local crate="$1" version="$2" attempt=0 output

  if is_already_published "$crate" "$version"; then
    log "SKIP ${crate} v${version}: already on crates.io"
    return 0
  fi

  while (( attempt < MAX_ATTEMPTS )); do
    attempt=$((attempt + 1))
    log "Publishing ${crate} v${version} (attempt ${attempt}/${MAX_ATTEMPTS})..."
    output="$(cargo publish --allow-dirty -p "$crate" 2>&1)"
    echo "$output"

    if echo "$output" | grep -q "Published ${crate} v${version}"; then
      log "OK ${crate} v${version} published"
      return 0
    fi
    if echo "$output" | grep -Eqi "already (published|uploaded|exists)|crate .* already"; then
      log "SKIP ${crate} v${version}: server reports it already exists"
      return 0
    fi
    if echo "$output" | grep -q "429"; then
      # Server tells us when to retry, e.g. "try again after Mon, 14 Sep 2026 11:33:44 GMT"
      local retry_after wait now target
      retry_after="$(echo "$output" | grep -oP 'try again after \K[A-Za-z]+, [0-9]+ [A-Za-z]+ [0-9]+ [0-9:]+ [A-Z]+' | head -1)"
      if [[ -n "$retry_after" ]]; then
        now="$(date -u +%s)"
        target="$(date -u -d "$retry_after" +%s)"
        wait=$((target - now + RETRY_BUFFER))
        (( wait < RETRY_BUFFER )) && wait=$RETRY_BUFFER
      else
        wait=600
      fi
      log "Rate limited (429). Sleeping ${wait}s before retry..."
      sleep "$wait"
      continue
    fi
    # Any other failure (e.g. dependency not yet visible on the index):
    # back off and retry.
    log "Publish of ${crate} failed with unexpected error; retrying in ${SETTLE_SECONDS}s..."
    sleep "$SETTLE_SECONDS"
  done

  log "ERROR giving up on ${crate} after ${MAX_ATTEMPTS} attempts"
  return 1
}

main() {
  cd "$WORKSPACE_ROOT"
  local start_at now wait
  now="$(date -u +%s)"
  start_at=$((now + DELAY_SECONDS))
  log "Scheduled publish run. Waiting ${DELAY_SECONDS}s; first publish at ~$(date -u -d "@${start_at}" '+%Y-%m-%d %H:%M:%S UTC')."
  sleep "$DELAY_SECONDS"

  local failed=0 crate version
  for crate in "${CRATES[@]}"; do
    version="$(crate_version "$crate")"
    if ! publish_one "$crate" "$version"; then
      failed=1
      # Continue with remaining crates anyway; dependents will retry
      # against the index and fail this run if their dep never landed.
    fi
    log "Settling ${SETTLE_SECONDS}s for index propagation..."
    sleep "$SETTLE_SECONDS"
  done

  if (( failed )); then
    log "DONE with failures. Re-run this script (it skips published crates); use a small delay, e.g. bash scripts/publish_crates_io.sh 300"
    return 1
  fi
  log "DONE all crates published successfully."
}

main "$@"
