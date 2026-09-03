# syntax=docker/dockerfile:1
#
# One shared builder stage compiles every Rust binary in the workspace in
# a single `cargo build`, so common dependencies (tokio, sqlx, reqwest,
# ...) are compiled ONCE instead of once per adapter. BuildKit cache
# mounts persist the cargo registry and the target dir across separate
# `docker compose build` runs, so incremental rebuilds after a small code
# change are fast too - only what actually changed gets recompiled.
#
# Each service in docker-compose.yml points at the SAME Dockerfile with a
# different `target:` (one per binary below) - BuildKit builds the shared
# `builder` stage once and reuses it for all of them.

FROM rust:1.98-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --workspace --bins && \
    mkdir -p /build/bin && \
    cp target/release/poms-ingestion \
       target/release/ellevio-adapter \
       target/release/vattenfall-adapter \
       target/release/kraftringen-adapter \
       target/release/tekniskaverken-adapter \
       target/release/oresundskraft-adapter \
       target/release/digpro-adapter \
       target/release/tekla-adapter \
       target/release/servicealert-adapter \
       target/release/malarenergi-adapter \
       target/release/upplandsenergi-adapter \
       target/release/voe-adapter \
       target/release/eksjo-adapter \
       target/release/piteenergi-adapter \
       target/release/karlshamn-adapter \
       target/release/skekraft-adapter \
       /build/bin/

# --- one thin final stage per binary ---

FROM debian:bookworm-slim AS runtime-base
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

FROM runtime-base AS ingestion
COPY --from=builder /build/bin/poms-ingestion /usr/local/bin/poms-ingestion
ENTRYPOINT ["/usr/local/bin/poms-ingestion"]

FROM runtime-base AS ellevio-adapter
COPY --from=builder /build/bin/ellevio-adapter /usr/local/bin/ellevio-adapter
ENTRYPOINT ["/usr/local/bin/ellevio-adapter"]

FROM runtime-base AS vattenfall-adapter
COPY --from=builder /build/bin/vattenfall-adapter /usr/local/bin/vattenfall-adapter
ENTRYPOINT ["/usr/local/bin/vattenfall-adapter"]

FROM runtime-base AS kraftringen-adapter
COPY --from=builder /build/bin/kraftringen-adapter /usr/local/bin/kraftringen-adapter
ENTRYPOINT ["/usr/local/bin/kraftringen-adapter"]

FROM runtime-base AS tekniskaverken-adapter
COPY --from=builder /build/bin/tekniskaverken-adapter /usr/local/bin/tekniskaverken-adapter
ENTRYPOINT ["/usr/local/bin/tekniskaverken-adapter"]

FROM runtime-base AS oresundskraft-adapter
COPY --from=builder /build/bin/oresundskraft-adapter /usr/local/bin/oresundskraft-adapter
ENTRYPOINT ["/usr/local/bin/oresundskraft-adapter"]

FROM runtime-base AS digpro-adapter
COPY --from=builder /build/bin/digpro-adapter /usr/local/bin/digpro-adapter
ENTRYPOINT ["/usr/local/bin/digpro-adapter"]

FROM runtime-base AS tekla-adapter
COPY --from=builder /build/bin/tekla-adapter /usr/local/bin/tekla-adapter
ENTRYPOINT ["/usr/local/bin/tekla-adapter"]

FROM runtime-base AS servicealert-adapter
COPY --from=builder /build/bin/servicealert-adapter /usr/local/bin/servicealert-adapter
ENTRYPOINT ["/usr/local/bin/servicealert-adapter"]

FROM runtime-base AS malarenergi-adapter
COPY --from=builder /build/bin/malarenergi-adapter /usr/local/bin/malarenergi-adapter
ENTRYPOINT ["/usr/local/bin/malarenergi-adapter"]

FROM runtime-base AS upplandsenergi-adapter
COPY --from=builder /build/bin/upplandsenergi-adapter /usr/local/bin/upplandsenergi-adapter
ENTRYPOINT ["/usr/local/bin/upplandsenergi-adapter"]

FROM runtime-base AS voe-adapter
COPY --from=builder /build/bin/voe-adapter /usr/local/bin/voe-adapter
ENTRYPOINT ["/usr/local/bin/voe-adapter"]

FROM runtime-base AS eksjo-adapter
COPY --from=builder /build/bin/eksjo-adapter /usr/local/bin/eksjo-adapter
ENTRYPOINT ["/usr/local/bin/eksjo-adapter"]

FROM runtime-base AS piteenergi-adapter
COPY --from=builder /build/bin/piteenergi-adapter /usr/local/bin/piteenergi-adapter
ENTRYPOINT ["/usr/local/bin/piteenergi-adapter"]

FROM runtime-base AS karlshamn-adapter
COPY --from=builder /build/bin/karlshamn-adapter /usr/local/bin/karlshamn-adapter
ENTRYPOINT ["/usr/local/bin/karlshamn-adapter"]

FROM runtime-base AS skekraft-adapter
COPY --from=builder /build/bin/skekraft-adapter /usr/local/bin/skekraft-adapter
ENTRYPOINT ["/usr/local/bin/skekraft-adapter"]
