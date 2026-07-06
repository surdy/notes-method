# syntax=docker/dockerfile:1
# ── Stage 1: build frontend ─────────────────────────────────────────────────
FROM node:22-bookworm AS frontend-builder

# Install pnpm
RUN corepack enable && corepack prepare pnpm@latest --activate

WORKDIR /src

# Layer-cache node_modules: copy lockfile first so this layer is only
# invalidated when dependencies change, not on every source edit.
COPY ui/app/package.json ui/app/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

# Build the SvelteKit app
COPY ui/app/ ./
RUN pnpm build

# ── Stage 2: build Rust binary ───────────────────────────────────────────────
FROM rust:1-bookworm AS rust-builder

# Cargo features to compile into the CLI binary. Empty by default so the lean
# `api`/`app` flavors stay small (no ONNX). The embed flavors pass
# `--build-arg CARGO_FEATURES=local-embed`, which pulls in fastembed/ort
# (ONNX Runtime), hf-hub, and tokenizers (ADR 0018 §9.2).
ARG CARGO_FEATURES=""

# Install cross-compilation target for linux/amd64 (no-op when already on amd64)
RUN rustup target add x86_64-unknown-linux-gnu

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build only the CLI crate (notesmith-tauri is excluded from workspace members).
# `${CARGO_FEATURES:+--features ${CARGO_FEATURES}}` expands to nothing when the
# arg is empty, keeping the lean build byte-for-byte unchanged.
RUN cargo build --release -p notesmith-cli --target x86_64-unknown-linux-gnu \
    ${CARGO_FEATURES:+--features ${CARGO_FEATURES}}

# ── Stage 3: api — runtime with Rust binary only ────────────────────────────
# Use this image when you access Notesmith through CLI/MCP/API clients or the
# Tauri desktop app with NOTESMITH_DESKTOP_DAEMON_URL. No browser-based UI is
# bundled; browser access to /app/ requires the 'app' flavor below.
FROM debian:bookworm-slim AS api

# ca-certificates: needed for any HTTPS calls the daemon may make
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Run as a non-root user
RUN groupadd --gid 1000 notesmith && \
    useradd --uid 1000 --gid 1000 --create-home notesmith
USER notesmith

COPY --from=rust-builder \
    /src/target/x86_64-unknown-linux-gnu/release/notesmith \
    /usr/local/bin/notesmith

# Directories that will be bind-mounted at runtime:
#   /vaults        - vault markdown files
#   /config        - notesmith global config (config.toml)
#   /data          - daemon state (lockfile, SQLite caches, Tantivy indexes)
#   /logs          - daemon log files
VOLUME ["/vaults", "/config", "/data", "/logs"]

# Expose the daemon HTTP port
EXPOSE 27183

# XDG env vars point into the bind-mount paths so config/data/logs are
# persisted on the host regardless of the container's home directory.
ENV XDG_CONFIG_HOME=/config \
    XDG_DATA_HOME=/data \
    XDG_STATE_HOME=/logs \
    XDG_RUNTIME_DIR=/data/run \
    RUST_LOG=info

COPY --chown=notesmith:notesmith deploy/healthcheck.sh /usr/local/bin/healthcheck.sh
RUN chmod +x /usr/local/bin/healthcheck.sh

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD ["/usr/local/bin/healthcheck.sh"]

ENTRYPOINT ["notesmith"]
CMD ["daemon", "start", "--bind", "0.0.0.0:27183"]

# ── Stage 4: app — runtime with frontend bundled (default) ──────────────────
# Includes the SvelteKit frontend. The daemon serves it at /app so browsers on
# the same network can use the full UI.
FROM api AS app

USER root
COPY --from=frontend-builder --chown=notesmith:notesmith /src/build /app-ui
USER notesmith

# Tell the daemon where to find the pre-built frontend assets.
ENV NOTESMITH_APP_DIR=/app-ui

# ── Stage 5: api-embed — API runtime, embed-capable ─────────────────────────
# Same as `api` but built with `--features local-embed` (pass
# `--build-arg CARGO_FEATURES=local-embed`). Ships the real fastembed/ONNX
# runtime so per-vault `[embed] enabled = true` can take effect (ADR 0018 §9.2).
#
# ONNX Runtime is statically linked into the binary by `ort`/fastembed, but it
# still dynamically links libgomp1 (OpenMP) and libstdc++6 at runtime, so those
# are installed here (they are NOT in the lean image, keeping it small).
#
# Model provisioning: the bge-small-en-v1.5 model is NOT baked in. On first use
# it is downloaded from HuggingFace into <data_dir>/models/. This image
# therefore needs outbound network access on first run; pre-seed that directory
# for air-gapped deployments.
FROM api AS api-embed

USER root
RUN apt-get update && \
    apt-get install -y --no-install-recommends libgomp1 libstdc++6 && \
    rm -rf /var/lib/apt/lists/*
USER notesmith

# ── Stage 6: app-embed — embed-capable runtime with frontend bundled ────────
# The embed-capable counterpart of `app`: fastembed/ONNX runtime plus the
# SvelteKit frontend served at /app.
FROM api-embed AS app-embed

USER root
COPY --from=frontend-builder --chown=notesmith:notesmith /src/build /app-ui
USER notesmith

ENV NOTESMITH_APP_DIR=/app-ui
