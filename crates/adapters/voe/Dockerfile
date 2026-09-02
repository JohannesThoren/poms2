FROM rust:1.82-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p poms-adapter-voe

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/voe-adapter /usr/local/bin/voe-adapter
ENTRYPOINT ["/usr/local/bin/voe-adapter"]
