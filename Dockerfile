FROM docker.io/library/rust:1.85.1-slim-bookworm AS builder 

ARG TARGETARCH

RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY proto ./proto
COPY src ./src
COPY common ./common

RUN cargo build --release

FROM docker.io/library/debian:bookworm-slim AS runner 

ARG TARGETARCH


RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    ca-certificates \
    curl \
    gnupg \
    libssl3 \
    && update-ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
    && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_20.x nodistro main" | tee /etc/apt/sources.list.d/nodesource.list \
    && apt-get update \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/mcp-gateway /usr/local/bin/mcp-gateway
COPY config.json /etc/mcp-gateway/config.json

ENTRYPOINT ["mcp-gateway", "-c", "/etc/mcp-gateway/config.json"]
