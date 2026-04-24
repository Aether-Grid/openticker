# syntax=docker/dockerfile:1.7

FROM node:22-bookworm-slim AS ui-builder
WORKDIR /app/crates/openticker-http/ui
ENV CI=true

COPY crates/openticker-http/ui/package.json crates/openticker-http/ui/pnpm-lock.yaml crates/openticker-http/ui/pnpm-workspace.yaml ./
RUN corepack enable \
    && corepack prepare pnpm@10.33.0 --activate \
    && pnpm install --frozen-lockfile

COPY crates/openticker-http/ui ./
RUN pnpm build

FROM rust:1-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        git \
        libsqlite3-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Space-separated list of Cargo features to enable on the `openticker-cli`
# build (e.g. `indicators` to pull in the private indicator pack). Leave empty
# for the pure OSS build.
ARG OPENTICKER_FEATURES=""
ARG GITHUB_TOKEN=""

COPY . .
COPY --from=ui-builder /app/crates/openticker-http/ui/.output /app/crates/openticker-http/ui/.output
RUN git submodule sync --recursive \
    && if [ -n "$GITHUB_TOKEN" ]; then \
        git \
            -c url."https://x-access-token:${GITHUB_TOKEN}@github.com/".insteadOf="https://github.com/" \
            -c url."https://x-access-token:${GITHUB_TOKEN}@github.com/".insteadOf="git@github.com:" \
            submodule update --init --recursive; \
    else \
        git submodule update --init --recursive; \
    fi \
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
