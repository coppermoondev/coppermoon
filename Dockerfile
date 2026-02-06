# ─── Build stage ──────────────────────────────────────────────────────
FROM rust:1-bookworm AS builder

WORKDIR /build

# Copy workspace
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY packages/ packages/

# Build release binaries
RUN cargo build --release \
    && strip target/release/coppermoon \
    && strip target/release/harbor \
    && strip target/release/shipyard

# ─── Runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 git \
    && rm -rf /var/lib/apt/lists/*

# Install CopperMoon binaries
COPY --from=builder /build/target/release/coppermoon /usr/local/bin/
COPY --from=builder /build/target/release/harbor /usr/local/bin/
COPY --from=builder /build/target/release/shipyard /usr/local/bin/

# Install scripts
COPY installer/ /installer/

# App directory
WORKDIR /app

EXPOSE 3000

CMD ["coppermoon"]
