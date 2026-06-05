# syntax=docker/dockerfile:1
# ── Stage 1: build ──────────────────────────────────────────────────────────
FROM rust:1-bookworm AS builder

# Install cross-compilation target for linux/amd64 (no-op when already on amd64)
RUN rustup target add x86_64-unknown-linux-gnu

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build only the CLI crate (notesmith-tauri is excluded from workspace members)
RUN cargo build --release -p notesmith-cli --target x86_64-unknown-linux-gnu

# ── Stage 2: runtime ────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# ca-certificates: needed for any HTTPS calls the daemon may make
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Run as a non-root user
RUN groupadd --gid 1000 notesmith && \
    useradd --uid 1000 --gid 1000 --create-home notesmith
USER notesmith

COPY --from=builder \
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
