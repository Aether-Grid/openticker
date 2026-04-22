# syntax=docker/dockerfile:1.7

FROM rust:1-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo test --workspace
RUN cargo build --release -p openticker-cli

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/openticker-cli /usr/local/bin/openticker-cli
COPY --from=builder /app/config /app/config

RUN mkdir -p /app/config /app/var

EXPOSE 8080

ENTRYPOINT ["openticker-cli"]
CMD ["service", "run", "--config-dir", "/app/config", "--bind", "0.0.0.0:8080"]
