FROM docker.io/library/node:23.11.0-bookworm AS node

WORKDIR /app

COPY ui .

RUN npm install

RUN npm run build

FROM docker.io/library/rust:1.86.0-slim-bookworm AS builder 

ARG TARGETARCH

RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler make libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Makefile Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY common ./common
COPY --from=node /app/out ./ui/out
RUN --mount=type=cache,id=cargo,target=/usr/local/cargo/registry cargo fetch --locked
RUN --mount=type=cache,id=cargo,target=/usr/local/cargo/registry make build

RUN strip target/release/agentgateway

FROM gcr.io/distroless/cc-debian12 AS runner 

ARG TARGETARCH
WORKDIR /app

COPY --from=builder /app/target/release/agentgateway /app/agentgateway

LABEL org.opencontainers.image.source=https://github.com/agentgateway/agentgateway
LABEL org.opencontainers.image.description="Agent Gateway is an agentic network gateway."

ENTRYPOINT ["/app/agentgateway"]
