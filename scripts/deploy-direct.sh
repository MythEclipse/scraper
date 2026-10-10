#!/usr/bin/env bash
#
# Direct cargo + systemd deploy for scraper.
#
# .github/workflows/deploy.yml scps THIS file out of the checkout of the commit
# being deployed and runs it on the VPS as:
#
#     bash /tmp/scraper-deploy-direct.sh <git-sha> <ref>
#
# The script is shipped from the checkout (never read from the running payload)
# so the deploy logic always matches the commit that is being released.
#
# Flow:
#   1. clone <git-sha> into $RELEASES_DIR/<git-sha>
#   2. cargo build --release --locked ON THE TARGET (glibc match with the host)
#   3. copy the binary + scrape_media.py into the launcher dir
#   4. restart the systemd unit
#   5. health-check GET /health with curl --retry (no sleep loops)
#   6. on failure: restore the previous binary, restart, re-check
#   7. prune old releases (keeps $KEEP_RELEASES newest, never the active one)
#
# Nix store paths are gone from this host, so the release directory is the only
# unit of deployment and `current` is the symlink the systemd unit reads through.
#
# Overrides (defaults are the production values; used for verification runs):
#   RELEASES_DIR LAUNCHER_DIR CURRENT_LINK UNIT PORT HEALTH_URL DEPLOY_REPO_URL
#   KEEP_RELEASES SKIP_RESTART=1
#
#   ARTIFACT_DIR     — directory holding the binary CI built (scraper +
#                      scrape_media.py). When set, no checkout and no build
#                      happen here. This is how CI invokes it.
set -Eeuo pipefail

REPO_URL="${DEPLOY_REPO_URL:-https://github.com/asepharyana/scraper.git}"
RELEASES_DIR="${RELEASES_DIR:-/opt/scraper/releases}"
LAUNCHER_DIR="${LAUNCHER_DIR:-/opt/scraper/bin}"
CURRENT_LINK="${CURRENT_LINK:-/opt/scraper/current}"
PREV_DIR="${CURRENT_LINK}.previous"
UNIT="${UNIT:-scraper}"
PORT="${PORT:-4091}"
HEALTH_URL="${HEALTH_URL:-http://127.0.0.1:${PORT}/health}"
# Total on disk, live included: 2 = the live release + one rollback target.
KEEP_RELEASES="${KEEP_RELEASES:-2}"
SKIP_RESTART="${SKIP_RESTART:-0}"

log() { printf '[deploy] %s\n' "$*"; }
die() { printf '[deploy] ERROR: %s\n' "$*" >&2; exit 1; }

SHA="${1:-}"
REF="${2:-main}"
[ -n "$SHA" ] || die "usage: $0 <git-sha> [ref]"

# ── privileges ─────────────────────────────────────────────────────────────
# Writes under /opt/scraper and systemctl need root when the deploy user is not
# root itself; use passwordless sudo when available, plain commands otherwise.
as_root() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  elif sudo -n true 2>/dev/null; then
    sudo -n "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    "$@"
  fi
}

# ── toolchain ──────────────────────────────────────────────────────────────
# The release binary is COMPILED BY CI and shipped as an artifact, so this host
# needs no Rust toolchain at all. That is the whole point: a release dir used to
# be a full checkout with a 1GB target/ in it, so two retained releases cost 2GB
# to keep 50MB of usable binaries.
#
# `ARTIFACT_DIR` is the directory CI uploads the built binary into. When it
# holds a binary, deploy uses it and skips the build entirely.
#
# Fallback: with no artifact, the script still builds locally, so a manual
# deploy (or a CI run that somehow skipped the build job) keeps working. That
# path is what leaves a target/ behind, and the prune step at the end removes it.
ARTIFACT_DIR="${ARTIFACT_DIR:-}"
CARGO_HOME_DIR="${CARGO_HOME:-/home/code/.cargo}"
RUSTUP_HOME_DIR="${RUSTUP_HOME:-/home/code/.rustup}"

resolve_cargo() {
  if command -v cargo >/dev/null 2>&1 && cargo --version >/dev/null 2>&1; then
    command -v cargo
  elif [ -x "${CARGO_HOME_DIR}/bin/cargo" ]; then
    printf '%s\n' "${CARGO_HOME_DIR}/bin/cargo"
  elif [ -x /usr/local/cargo/bin/cargo ]; then
    printf '%s\n' /usr/local/cargo/bin/cargo
  else
    return 1
  fi
}

CARGO=""
command -v git  >/dev/null 2>&1 || die "git not found on PATH"
command -v curl >/dev/null 2>&1 || die "curl not found on PATH"
CARGO="$(resolve_cargo)" || CARGO=""
log "user $(id -un) | cargo: ${CARGO:-<absent>} | artifact: ${ARTIFACT_DIR:-<none>}"

command -v unzip >/dev/null 2>&1 || die "unzip not found on PATH (needed to unpack the CI artifact)"

# ── state captured before we touch anything ────────────────────────────────
PREV_TARGET=""
if [ -L "$CURRENT_LINK" ]; then
  PREV_TARGET="$(readlink "$CURRENT_LINK")"
elif [ -d "$CURRENT_LINK" ]; then
  # First deploy: /opt/scraper/current is a real directory (pre-direct payload).
  PREV_TARGET="$PREV_DIR"
fi
BACKUP_BIN="${LAUNCHER_DIR}/.scraper.bin.pre"
ACTIVATED=0

# Restore the exact binary that was live before this deploy. Kept out of the
# rollback closure's error path (`set +e`) on purpose: a rollback that aborts is
# worse than a rollback that reports.
rollback() {
  set +e
  trap - ERR
  log "ROLLING BACK"
  if [ -f "$BACKUP_BIN" ]; then
    as_root cp -f "$BACKUP_BIN" "${LAUNCHER_DIR}/scraper"
    as_root chmod 0555 "${LAUNCHER_DIR}/scraper"
    log "restored previous binary from ${BACKUP_BIN}"
  elif [ -n "$PREV_TARGET" ] && [ -x "${PREV_TARGET}/scraper" ]; then
    # Payload release (normal): the binary sits at the release root.
    as_root cp -f "${PREV_TARGET}/scraper" "${LAUNCHER_DIR}/scraper"
    as_root chmod 0555 "${LAUNCHER_DIR}/scraper"
    log "restored previous binary from $PREV_TARGET"
  elif [ -n "$PREV_TARGET" ] && [ -x "${PREV_TARGET}/target/release/scraper" ]; then
    # Source checkout (releases made before the artifact switch).
    as_root cp -f "${PREV_TARGET}/target/release/scraper" "${LAUNCHER_DIR}/scraper"
    as_root chmod 0555 "${LAUNCHER_DIR}/scraper"
    log "restored previous binary from $PREV_TARGET (source checkout)"
  else
    log "no previous binary to restore (nothing to roll back to)"
  fi
  if [ "$SKIP_RESTART" != "1" ]; then
    as_root systemctl restart "$UNIT"
    health_check || log "WARNING: service still unhealthy after rollback"
  fi
}

# $? must be captured before on_error runs: the trap receives only the line
# number, so on_error's first arg would otherwise be the LINE, not the exit
# status, and `set -u` on an absent $2 would abort the trap mid-way — leaving a
# failed deploy to continue as if it had succeeded.
on_error() {
  local rc="$1" line="$2"
  log "step failed at line $line (exit $rc)"
  if [ "$ACTIVATED" = "1" ]; then
    rollback
  fi
  exit "$rc"
}
trap 'rc=$?; on_error "$rc" "$LINENO"' ERR

health_check() {
  log "health-check $HEALTH_URL"
  curl --fail --silent --show-error \
    --retry 15 --retry-delay 2 \
    --retry-connrefused --retry-all-errors \
    --max-time 10 \
    -o /dev/null "$HEALTH_URL"
}

# ── 1. the release payload ─────────────────────────────────────────────────
# Two shapes, same downstream path:
#
#   artifact mode (normal)  — CI compiled the binary and uploaded it. The
#     release dir holds the payload: the binary, scrape_media.py, and nothing
#     else. No git checkout, no target/, no source.
#   source mode (fallback) — no artifact was supplied, so clone the SHA and
#     build here. Slower and leaves a target/ behind, but it keeps a manual
#     deploy working without touching CI.
#
# Either way the release dir ends up self-contained and small, because the
# binary is COPIED into $LAUNCHER_DIR below and nothing at runtime reads the
# release dir (verified: the crate reads /proc, .env and external paths, no
# embedded assets).
RELEASE_DIR="${RELEASES_DIR}/${SHA}"
PAYLOAD_ONLY=0

if [ -n "$ARTIFACT_DIR" ] && [ -x "${ARTIFACT_DIR}/scraper" ]; then
  log "artifact mode: using the binary CI built ($(stat -c '%s bytes' "${ARTIFACT_DIR}/scraper"))"
  as_root mkdir -p "$RELEASE_DIR"
  as_root cp -f "${ARTIFACT_DIR}/scraper" "${RELEASE_DIR}/scraper"
  # scrape_media.py is read at runtime by the Playwright fallback, so it must
  # ship next to the binary (src/infrastructure/repository/downloader/shared.rs).
  [ -f "${ARTIFACT_DIR}/scrape_media.py" ] \
    || die "CI artifact is missing scrape_media.py"
  as_root cp -f "${ARTIFACT_DIR}/scrape_media.py" "${RELEASE_DIR}/scrape_media.py"
  BIN_SRC="${RELEASE_DIR}/scraper"
  SCRIPT_SRC="${RELEASE_DIR}/scrape_media.py"
  PAYLOAD_ONLY=1
else
  [ -n "$ARTIFACT_DIR" ] && log "WARNING: ARTIFACT_DIR set but no binary in it — falling back to a host build"
  [ -n "$CARGO" ] || die "no CI artifact and no local cargo — cannot build scraper. Set ARTIFACT_DIR or install cargo."
  PAYLOAD_ONLY=0

  if [ -d "${RELEASE_DIR}/.git" ]; then
    log "reusing existing checkout $RELEASE_DIR"
  else
    if [ ! -d "$RELEASES_DIR" ]; then
      as_root mkdir -p "$RELEASES_DIR"
    fi
    if [ ! -w "$RELEASES_DIR" ]; then
      as_root chown "$(id -u):$(id -g)" "$RELEASES_DIR"
    fi
    log "cloning $SHA from $REPO_URL"
    rm -rf "$RELEASE_DIR"
    mkdir -p "$RELEASE_DIR"
    git -C "$RELEASE_DIR" init -q
    git -C "$RELEASE_DIR" remote add origin "$REPO_URL"
    if ! git -C "$RELEASE_DIR" fetch --quiet --depth 1 origin "$SHA" 2>/dev/null; then
      # Some servers refuse direct SHA fetches; fall back to the branch and walk
      # back to the commit. The depth is generous on purpose (SHAs land on main).
      log "direct SHA fetch refused; fetching $REF instead"
      git -C "$RELEASE_DIR" fetch --quiet --depth 500 origin \
        "+refs/heads/${REF}:refs/remotes/origin/${REF}" \
        || die "could not fetch $REF from $REPO_URL"
    fi
    git -C "$RELEASE_DIR" checkout --quiet --detach "$SHA" \
      || git -C "$RELEASE_DIR" checkout --quiet --detach FETCH_HEAD \
      || die "commit $SHA not found in $REPO_URL"
  fi
  [ -f "$RELEASE_DIR/Cargo.toml" ] || die "$RELEASE_DIR is not a checkout of scraper"
  log "checked out $(git -C "$RELEASE_DIR" rev-parse --short HEAD 2>/dev/null || echo "$SHA")"

  # ── 2. build here (fallback path only) ────────────────────────────────────
  # A native binary normally is compiled by CI. This exists so a manual deploy
  # still works, and so a CI run whose artifact step failed loudly here rather
  # than quietly shipping a stale binary.
  if [ -x "${RELEASE_DIR}/target/release/scraper" ] \
     && [ "${RELEASE_DIR}/target/release/scraper" -nt "${RELEASE_DIR}/Cargo.toml" ]; then
    log "release binary already built in $RELEASE_DIR — reusing"
  else
    export CARGO_HOME="$CARGO_HOME_DIR" RUSTUP_HOME="$RUSTUP_HOME_DIR"
    ( cd "$RELEASE_DIR" && log "cargo build --release --locked" && "$CARGO" build --release --locked )
  fi
  [ -x "${RELEASE_DIR}/target/release/scraper" ] || die "cargo build produced no release binary"
  BIN_SRC="${RELEASE_DIR}/target/release/scraper"
  log "binary: $(stat -c '%s bytes' "$BIN_SRC")"

  # scrape_media.py ships next to the binary: run_playwright_scraper() looks there
  # first, and that lookup is the only reason the Playwright fallback worked.
  [ -f "${RELEASE_DIR}/scrape_media.py" ] || die "scrape_media.py missing from $RELEASE_DIR"
  SCRIPT_SRC="${RELEASE_DIR}/scrape_media.py"
fi

[ -x "$BIN_SRC" ] || die "no release binary at $BIN_SRC"

# In fallback mode the build left a ~1GB target/ in the release. The binary is
# COPIED into $LAUNCHER_DIR by the activate step, and nothing reads the release
# dir at runtime, so it can go now rather than waiting for the prune — otherwise
# a manual deploy keeps a gigabyte per release for no reason.
if [ "$PAYLOAD_ONLY" = "0" ] && [ -d "${RELEASE_DIR}/target" ]; then
  log "dropping build cache ${RELEASE_DIR}/target ($(du -sh "${RELEASE_DIR}/target" 2>/dev/null | cut -f1))"
  as_root rm -rf "${RELEASE_DIR}/target"
fi

# ── 3. activate ────────────────────────────────────────────────────────────
# The binary is copied, not symlinked: systemd's ExecStart reads /opt/scraper/bin
# directly, and a half-written executable must never be the one a restart picks
# up. Copy to a temp name in the same dir, chmod, then rename(2) into place.
log "activating $RELEASE_DIR"
# LAUNCHER_DIR is created rather than assumed: a first deploy into a fresh dir
# failed on `cp: cannot create .../.scraper.new`.
as_root mkdir -p "$LAUNCHER_DIR"
if [ -f "$LAUNCHER_DIR/scraper" ]; then
  as_root cp -f "${LAUNCHER_DIR}/scraper" "$BACKUP_BIN"
fi
NEW_LINK="${CURRENT_LINK}.new.$$"
as_root ln -sfn "$RELEASE_DIR" "$NEW_LINK"
if [ -L "$CURRENT_LINK" ]; then
  as_root mv -Tf "$NEW_LINK" "$CURRENT_LINK"   # atomic rename(2)
elif [ -e "$CURRENT_LINK" ]; then
  as_root mv "$CURRENT_LINK" "$PREV_DIR"       # directory -> symlink, first run
  if ! as_root mv -Tf "$NEW_LINK" "$CURRENT_LINK"; then
    as_root mv "$PREV_DIR" "$CURRENT_LINK"
    die "could not activate $RELEASE_DIR"
  fi
else
  as_root mv -Tf "$NEW_LINK" "$CURRENT_LINK"
fi
# A payload release carries the binary at its root; a source checkout (the
# fallback path) has it under target/. Either satisfies this — asserting
# Cargo.toml would fail every artifact-mode deploy.
[ -x "${CURRENT_LINK}/scraper" ] \
  || [ -x "${CURRENT_LINK}/target/release/scraper" ] \
  || die "$CURRENT_LINK does not point at a release"

as_root cp -f "$BIN_SRC" "${LAUNCHER_DIR}/.scraper.new"
as_root chmod 0555 "${LAUNCHER_DIR}/.scraper.new"
as_root mv -f "${LAUNCHER_DIR}/.scraper.new" "${LAUNCHER_DIR}/scraper"
as_root cp -f "$SCRIPT_SRC" "${LAUNCHER_DIR}/scrape_media.py"
as_root chmod 0555 "${LAUNCHER_DIR}/scrape_media.py"
ACTIVATED=1
log "installed ${LAUNCHER_DIR}/scraper + scrape_media.py from $(basename "$RELEASE_DIR")"

# ── 4. restart ─────────────────────────────────────────────────────────────
if [ "$SKIP_RESTART" = "1" ]; then
  log "SKIP_RESTART=1 — not restarting $UNIT"
else
  log "restarting $UNIT"
  as_root systemctl restart "$UNIT"
fi

# ── 5. health check ────────────────────────────────────────────────────────
# A unit with Restart=always is `active` for the whole of its crash loop, so
# `systemctl is-active` alone would report a green deploy on a service that
# never came up. curl --retry against /health is the only honest signal.
if health_check; then
  ACTIVATED=0
  # as_root: the backup lives in LAUNCHER_DIR, which is root-owned on this host.
  # A bare `rm` here failed with EACCES under a non-root deploy user, aborting
  # the script AFTER a successful deploy and reporting the run as failed.
  as_root rm -f "$BACKUP_BIN" || log "WARNING: could not remove $BACKUP_BIN"
  log "healthy — deploy of ${SHA:0:7} complete"
else
  # `die` below would skip the ERR trap (plain exit), so roll back explicitly.
  rollback
  die "health check failed after deploying ${SHA:0:7}"
fi

# ── 6. prune old releases ──────────────────────────────────────────────────
# Keep the live release plus exactly ONE rollback target.
#
# A rollback is `ln -sfn releases/<prev> current && systemctl restart`, which
# needs only the single previous release on disk. Everything older is
# unreachable, so a higher cap buys nothing and costs a full checkout per
# deploy — this was 3 x ~1G for this project alone.
#
# Runs LAST, after the health check, and that ordering is load-bearing: pruning
# first would destroy the rollback target of the release you are rolling back
# TO, turning a failed deploy into an unrecoverable one.
#
# `current` is resolved to its basename before comparing: readlink returns the
# absolute path while the loop yields `dir/` with a trailing slash, so a string
# compare against the raw target can never match and the LIVE release would be
# pruned on mtime order alone.
#
# KEEP_RELEASES is the TOTAL on disk, live included — not "the newest N, plus
# the live one", which yields N+1 whenever the live release is older than the N
# newest, exactly the rollback case this cap exists to support.
log "pruning old releases (keeping live + 1 rollback)"
LIVE_SHA="$(basename "$(readlink -f "$CURRENT_LINK" 2>/dev/null || echo "")")"
[ -n "$LIVE_SHA" ] || die "could not resolve the live release from $CURRENT_LINK"

mapfile -t KEEP < <(
  ls -1dt "$RELEASES_DIR"/*/ 2>/dev/null \
    | while read -r d; do printf '%s\t%s\n' "$(stat -c %Y "$d")" "$(basename "$d")"; done \
    | sort -rn \
    | cut -f2 \
    | while read -r name; do
        [ "$name" = "$LIVE_SHA" ] && continue
        printf '%s\n' "$name"
      done \
    | head -n "$((KEEP_RELEASES - 1))"
)
log "  keeping: live=$LIVE_SHA + ${KEEP[*]:-<none>}"

for dir in "$RELEASES_DIR"/*/; do
  [ -d "$dir" ] || continue
  name="$(basename "$dir")"
  [ "$name" = "$LIVE_SHA" ] && continue
  [ "$name" = "$(basename "$RELEASE_DIR")" ] && continue
  if printf '%s\n' "${KEEP[@]:-}" | grep -qx "$name"; then
    continue
  fi
  log "  pruning $name ($(du -sh "$dir" 2>/dev/null | cut -f1))"
  as_root rm -rf "$dir"
done
log "  releases now: $(ls -1 "$RELEASES_DIR" 2>/dev/null | wc -l)"

log "done: $CURRENT_LINK -> $LIVE_SHA"