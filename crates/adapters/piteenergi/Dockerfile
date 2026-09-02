FROM rust:1.82-slim AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p poms-adapter-piteenergi

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/piteenergi-adapter /usr/local/bin/piteenergi-adapter
ENTRYPOINT ["/usr/local/bin/piteenergi-adapter"]
