FROM rust:1.85.0-slim-bullseye AS builder 

ARG TARGETARCH

COPY Cargo.toml Cargo.lock ./

RUN cargo update

COPY main.rs ./

RUN cargo build --release

FROM ghcr.io/astral-sh/uv:0.6.5-debian-slim AS runner 

ARG TARGETARCH


RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    ca-certificates \
    curl \
    gnupg \
    && update-ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
    && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_20.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list \
    && apt-get update \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*


RUN uv python install 3.12

COPY --from=builder /target/release/mcp-relay /usr/local/bin/mcp-relay

ENTRYPOINT ["mcp-relay"]
