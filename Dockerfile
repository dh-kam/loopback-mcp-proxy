FROM node:22-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.94.1-bookworm AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY .cargo ./.cargo
COPY scripts/release-musl.sh ./scripts/release-musl.sh
COPY src ./src
COPY --from=web /src/web/dist ./web/dist
RUN chmod +x ./scripts/release-musl.sh \
    && ./scripts/release-musl.sh \
    && mkdir -p /out \
    && cp build/linux-*/release/loopback-mcp-proxy /out/loopback-mcp-proxy

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/loopback-mcp-proxy /usr/local/bin/loopback-mcp-proxy
ENV BIND=0.0.0.0:8080 \
    DATA_DIR=/data \
    ALLOW_NON_LOOPBACK=1 \
    PUBLIC_BASE_URL=http://127.0.0.1:8080 \
    RUST_LOG=info
VOLUME ["/data"]
EXPOSE 8080
ENTRYPOINT ["loopback-mcp-proxy"]
CMD ["serve"]
