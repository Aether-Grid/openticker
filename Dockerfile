# syntax=docker/dockerfile:1.7

FROM rust:1-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        git \
        libsqlite3-dev \
        nodejs \
        npm \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Space-separated list of Cargo features to enable on the `openticker-cli`
# build (e.g. `indicators` to pull in the private indicator pack). Leave empty
# for the pure OSS build.
ARG OPENTICKER_FEATURES=""
ARG GITHUB_TOKEN=""

COPY . .
RUN git submodule sync --recursive \
    && if [ -n "$GITHUB_TOKEN" ]; then \
        git \
            -c url."https://x-access-token:${GITHUB_TOKEN}@github.com/".insteadOf="https://github.com/" \
            -c url."https://x-access-token:${GITHUB_TOKEN}@github.com/".insteadOf="git@github.com:" \
            submodule update --init --recursive; \
    else \
        git submodule update --init --recursive; \
    fi \
    && npm install -g pnpm@10.33.0 \
    && cd crates/openticker-http/ui \
    && pnpm install --frozen-lockfile \
    && pnpm build \
    && cd /app \
    && cargo test --workspace ${OPENTICKER_FEATURES:+--features "$OPENTICKER_FEATURES"} \
    && cargo build --release -p openticker-cli ${OPENTICKER_FEATURES:+--features "$OPENTICKER_FEATURES"}

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
