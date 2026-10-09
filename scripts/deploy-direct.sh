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
set -Eeuo pipefail

REPO_URL="${DEPLOY_REPO_URL:-https://github.com/asepharyana/scraper.git}"
RELEASES_DIR="${RELEASES_DIR:-/opt/scraper/releases}"
LAUNCHER_DIR="${LAUNCHER_DIR:-/opt/scraper/bin}"
CURRENT_LINK="${CURRENT_LINK:-/opt/scraper/current}"
PREV_DIR="${CURRENT_LINK}.previous"
UNIT="${UNIT:-scraper}"
PORT="${PORT:-4091}"
HEALTH_URL="${HEALTH_URL:-http://127.0.0.1:${PORT}/health}"
KEEP_RELEASES="${KEEP_RELEASES:-5}"
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
command -v cargo >/dev/null 2>&1 || die "cargo not found on PATH (expected a rustup toolchain)"
command -v git   >/dev/null 2>&1 || die "git not found on PATH"
command -v curl  >/dev/null 2>&1 || die "curl not found on PATH"
log "cargo $(cargo --version) | user $(id -un)"

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
  elif [ -n "$PREV_TARGET" ] && [ -e "$PREV_TARGET/target/release/scraper" ]; then
    as_root cp -f "${PREV_TARGET}/target/release/scraper" "${LAUNCHER_DIR}/scraper"
    as_root chmod 0555 "${LAUNCHER_DIR}/scraper"
    log "restored previous binary from ${PREV_TARGET}"
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

# ── 1. check out the release ───────────────────────────────────────────────
RELEASE_DIR="${RELEASES_DIR}/${SHA}"
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
# `|| echo "$SHA"` matters: this is a diagnostic, and under the ERR trap a
# failing command substitution inside log() would abort an otherwise good deploy.
log "checked out $(git -C "$RELEASE_DIR" rev-parse --short HEAD 2>/dev/null || echo "$SHA")"

# ── 2. build on the target ─────────────────────────────────────────────────
# A native binary must be compiled where it runs: Ubuntu 26.04 glibc, and the
# openssl-sys link against the host's libssl. A CI-runner artifact would risk a
# glibc mismatch that only shows up as a failed execve on restart.
if [ -x "${RELEASE_DIR}/target/release/scraper" ] \
   && [ "${RELEASE_DIR}/target/release/scraper" -nt "${RELEASE_DIR}/Cargo.toml" ]; then
  log "release binary already built in $RELEASE_DIR — reusing"
else
  (
    cd "$RELEASE_DIR"
    log "cargo build --release --locked"
    cargo build --release --locked
  )
fi
[ -x "${RELEASE_DIR}/target/release/scraper" ] || die "cargo build produced no release binary"
BIN_SRC="${RELEASE_DIR}/target/release/scraper"
log "binary: $(stat -c '%s bytes' "$BIN_SRC")"

# scrape_media.py ships next to the binary: run_playwright_scraper() looks there
# first (src/infrastructure/repository/downloader/shared.rs), and that lookup is
# the only reason the Playwright fallback worked under Nix, whose installPhase
# copied it into $out/bin.
[ -f "${RELEASE_DIR}/scrape_media.py" ] || die "scrape_media.py missing from $RELEASE_DIR"
SCRIPT_SRC="${RELEASE_DIR}/scrape_media.py"

# ── 3. activate ────────────────────────────────────────────────────────────
# The binary is copied, not symlinked: systemd's ExecStart reads /opt/scraper/bin
# directly, and a half-written executable must never be the one a restart picks
# up. Copy to a temp name in the same dir, chmod, then rename(2) into place.
log "activating $RELEASE_DIR"
if [ -f "${LAUNCHER_DIR}/scraper" ]; then
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
[ -e "${CURRENT_LINK}/Cargo.toml" ] || die "$CURRENT_LINK does not point at a release"

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
  rm -f "$BACKUP_BIN"
  log "healthy — deploy of ${SHA:0:7} complete"
else
  # `die` below would skip the ERR trap (plain exit), so roll back explicitly.
  rollback
  die "health check failed after deploying ${SHA:0:7}"
fi

# ── 6. prune old releases ──────────────────────────────────────────────────
active_target="$(readlink "$CURRENT_LINK")"
kept=0
while IFS= read -r dir; do
  [ -n "$dir" ] || continue
  [ "$dir" = "$active_target" ] && continue
  [ "$dir" = "$RELEASE_DIR" ] && continue
  [ "$dir" = "$PREV_DIR" ] && continue
  kept=$((kept + 1))
  if [ "$kept" -gt "$KEEP_RELEASES" ]; then
    log "pruning old release $(basename "$dir")"
    as_root rm -rf "$dir"
  fi
done < <(ls -1dt "$RELEASES_DIR"/*/ 2>/dev/null || true)

log "done: $CURRENT_LINK -> $active_target"