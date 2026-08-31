# Multitool web server. Two-stage: build the Vite SPA + Rust server, then ship
# a slim runtime with one mounted path (/data) for all state.
#
#   docker build -t multitool-server .
#   docker run -p 8080:8080 -v multitool-data:/data \
#     -e PUBLIC_URL=https://multitool.example.org multitool-server
#
# Connect to RE NXT through the app's Settings → General panel. The RE_* env
# vars are an optional alternative for headless deploys; with neither, the
# server runs in mock mode against bundle fixtures.

# ── frontend ──────────────────────────────────────────────────────────────────
FROM node:20-bookworm-slim AS frontend
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY index.html vite.config.ts tsconfig.json tsconfig.node.json components.json ./
COPY public ./public
COPY src ./src
RUN npm run build

# ── server ────────────────────────────────────────────────────────────────────
FROM rust:1-bookworm AS backend
WORKDIR /app
# DuckDB is bundled (compiled from source) — needs a C++ toolchain, present in
# the rust image. Build only the server crate; the Tauri shell isn't needed.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY src-tauri/Cargo.toml ./src-tauri/Cargo.toml
COPY src-tauri/build.rs ./src-tauri/build.rs
# Stub the tauri member so the workspace resolves without the desktop sources.
RUN mkdir -p src-tauri/src && echo 'fn main() {}' > src-tauri/src/main.rs
# Built-in profiles are include_bytes!-embedded by crates/core.
COPY profiles ./profiles
RUN cargo build --release -p multitool-server

# ── runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /app/target/release/multitool-server ./multitool-server
COPY --from=frontend /app/dist ./dist
ENV DATA_DIR=/data STATIC_DIR=/app/dist BIND_ADDR=0.0.0.0:8080
VOLUME /data
EXPOSE 8080
CMD ["./multitool-server"]
