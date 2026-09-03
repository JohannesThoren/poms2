FROM rust:1.98-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p poms-adapter-skekraft

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/skekraft-adapter /usr/local/bin/skekraft-adapter
ENTRYPOINT ["/usr/local/bin/skekraft-adapter"]
