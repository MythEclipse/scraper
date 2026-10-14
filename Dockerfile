# syntax=docker/dockerfile:1
#
# scraper-service — Rust binary, built to run on the SAME glibc as production.
#
# GLIBC REASONING (this is the whole reason the base is not bookworm/alpine):
#
#   .github/workflows/deploy.yml documents the production host as Ubuntu 26.04
#   (glibc 2.43), and notes ubuntu-latest is 24.04 (glibc 2.39) — and that the
#   runner->host direction is SAFE because glibc is backward compatible (a
#   binary built against 2.39 runs on 2.43). The deploy path therefore builds on
#   a machine OLDER than production and runs on the newer host.
#
#   A Docker image inverts that: the image is the runtime. If the builder glibc
#   were newer than the image's own glibc, the binary would carry a
#   GLIBC_2.4x symbol the loader cannot resolve and would die with "version
#   `GLIBC_2.4x' not found" at execve — after the deploy succeeded.
#
#   So BOTH stages are ubuntu:26.04, whose glibc is 2.43 — identical to the
#   production host. There is no cross-version direction at all: the image is
#   built and runs against the same glibc, and it is byte-for-byte the same
#   environment CI's runner-produced binary is proven against. Debian bookworm
#   (glibc 2.36) was rejected for exactly this reason: a bookworm-built image
#   would be OLDER than production, which is safe, but it would also not be
#   "the same glibc as production" and would silently pin the service to the
#   oldest libc it can ever run on, forcing an image rebuild on every Ubuntu
#   bump the host takes. Alpine is worse: musl, a different libc entirely.
#   See glibc-check below, which fails the build if this ever drifts.

# ── build stage ─────────────────────────────────────────────────────────────
# ubuntu:26.04 (NOT rust:slim) so the compiler and the runtime share a libc.
FROM ubuntu:26.04 AS build

ARG RUST_VERSION=1.90.0

ENV DEBIAN_FRONTEND=noninteractive

# build-essential: cc/ld for the linker. pkg-config: openssl/zlib/zstd discovery.
# libssl-dev / libzstd-dev / zlib1g-dev: the release binary links libssl.so.3,
# libzstd.so.1 and libz.so.1 dynamically (verified with ldd on target/release),
# so these headers must exist at build time as well as the runtime .so files.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      build-essential \
      ca-certificates \
      pkg-config \
      libssl-dev \
      libzstd-dev \
      zlib1g-dev \
      curl \
 && rm -rf /var/lib/apt/lists/*

# rustup, pinned. Installed to /usr/local/cargo (build stage only — never in
# the runtime image).
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_VERSION" \
 && rustc --version && cargo --version

WORKDIR /build

# Dependency layer first, so a source-only change does not refetch the registry.
# This crate pulls ~200 crates (mytheclipse full + axum + sea-orm + tokio);
# keeping them in their own layer is what makes rebuilds bearable.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
 && echo 'fn main() {}' > src/main.rs \
 && echo '' > src/lib.rs \
 && cargo fetch --locked

COPY src ./src

# --locked: refuse to touch Cargo.lock, matching CI exactly.
# --release: opt-level 3 + fat LTO + codegen-units 1 + strip, as Cargo.toml asks.
#
# Deliberately no `--mount=type=cache` for the cargo registry or target/: cache
# mounts require BuildKit, and the stock Docker CLI in this environment (and on
# many VPSes) predates it — the build dies with "the --mount option requires
# BuildKit" before compiling anything. A plain RUN works on every builder, which
# matters more than incremental rebuild speed for a release image. If your CI
# has BuildKit, adding
#   --mount=type=cache,target=/usr/local/cargo/registry \
#   --mount=type=cache,target=/build/target \
# here is a safe, build-only optimisation that does not affect the output.
RUN cargo build --release --locked --bin scraper

# Prove the artifact is sound in the SAME image it will run in: resolve every
# dynamic library, and assert the libc it needs is the libc this image has.
# A missing libssl here would otherwise surface as a crash on first request.
#
# The libc assertion matches /(usr/)?lib/ rather than a literal /lib: Ubuntu is
# merged-/usr, so /lib is a symlink to /usr/lib and every library above resolved
# under /usr/lib/x86_64-linux-gnu. The literal form failed on a perfectly good
# binary, which is worse than no check at all.
#
# Comments cannot live inside a backslash-continued RUN: the shell receives them
# as commands and the parser reports `unknown instruction: echo`.
RUN ldd target/release/scraper \
 && for lib in libc.so.6 libssl.so.3 libcrypto.so.3 libz.so.1 libzstd.so.1 libgcc_s.so.1; do \
      ldd target/release/scraper | grep -q "$lib" \
        || { echo "FAIL: release binary no longer links $lib"; exit 1; }; \
    done \
 && printf 'image glibc: %s\n' "$(ldd --version | head -1)" \
 && if ! ldd target/release/scraper | grep -qE 'libc\.so\.6 => /(usr/)?lib/'; then \
      echo "FAIL: libc.so.6 not resolving to a standard library path"; exit 1; \
    fi

# ── runtime stage ───────────────────────────────────────────────────────────
FROM ubuntu:26.04 AS runtime

LABEL org.opencontainers.image.title="scraper-service" \
      org.opencontainers.image.description="PR review/agent scraper service (Rust)" \
      org.opencontainers.image.source="https://github.com/asepharyana/scraper"

ENV DEBIAN_FRONTEND=noninteractive

# Runtime shared libraries, matched to what `ldd` proved the binary needs:
#   libssl3t64 -> libssl.so.3 + libcrypto.so.3 (sea-orm/sqlx, via openssl-sys)
#   zlib1g     -> libz.so.1   (flate2)
#   libzstd1   -> libzstd.so.1 (zstd, via tower-http compression-zstd)
# ca-certificates: the service makes outbound HTTPS calls (GitHub, reqwest).
# python3: scrape_media.py is the Playwright fallback for yt-dlp-blocked URLs.
# tini: PID 1 that reaps and forwards SIGTERM.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      libssl3t64 \
      zlib1g \
      libzstd1 \
      ca-certificates \
      curl \
      python3 \
      tini \
 && rm -rf /var/lib/apt/lists/*

# Non-root. ubuntu:26.04 ships a `ubuntu` user at uid/gid 1000.
RUN groupadd --system --gid 1001 scraper \
 && useradd --system --uid 1001 --gid scraper --home-dir /app --shell /usr/sbin/nologin scraper

WORKDIR /app

# Binary AND scrape_media.py land in the SAME directory on purpose:
# downloader/shared.rs resolves the script via the directory of the running
# executable first, then falls back to CARGO_MANIFEST_DIR (a build-time path
# that does not exist in this image). Same-dir placement is what makes the
# Playwright fallback reachable at runtime.
COPY --from=build --chown=scraper:scraper /build/target/release/scraper /usr/local/bin/scraper
COPY --chown=scraper:scraper scrape_media.py /usr/local/bin/scrape_media.py

USER scraper

# default_port() in src/config/mod.rs is 4091; ecosystem.config.cjs and
# deploy.yml both use 4091. Expose it for documentation and `docker run -p`.
EXPOSE 4091

ENV PORT=4091 \
    APP_ENV=production

# HEALTHCHECK: the port is HARD-CODED as 4091. This form is executed by
# /bin/sh -c, where ${...} is POSIX parameter expansion — not JS — so a
# ${PORT} here would expand to empty and probe the wrong port. Poll the real
# observable condition (/health answering 200), never a sleep.
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -fsS --max-time 4 http://127.0.0.1:4091/health || exit 1

# exec form: scraper becomes PID 1 and receives SIGTERM directly. tini wraps it
# to reap the python/yt-dlp children it spawns and to forward signals.
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["scraper"]