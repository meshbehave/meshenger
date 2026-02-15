FROM rust:trixie AS builder

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

FROM node:22-trixie-slim AS web-builder

WORKDIR /web

COPY web/package.json web/package-lock.json ./
RUN npm ci

COPY web/index.html /web/index.html
COPY web/tsconfig.json /web/tsconfig.json
COPY web/tsconfig.app.json /web/tsconfig.app.json
COPY web/tsconfig.node.json /web/tsconfig.node.json
COPY web/vite.config.ts /web/vite.config.ts
COPY web/eslint.config.js /web/eslint.config.js
COPY web/public /web/public
COPY web/src /web/src

RUN npm run build

FROM debian:trixie-slim AS runtime

ARG APP_UID=1000
ARG APP_GID=1000

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --gid "${APP_GID}" meshenger \
    && useradd --uid "${APP_UID}" --gid meshenger --create-home --home-dir /home/meshenger --shell /usr/sbin/nologin meshenger

WORKDIR /app

COPY --from=builder /build/target/release/meshenger /usr/local/bin/meshenger
COPY --from=web-builder /web/dist /app/web/dist
COPY config.example.toml /app/config.example.toml

RUN mkdir -p /config /data \
    && chown -R meshenger:meshenger /app /config /data /home/meshenger

RUN printf '%s\n' \
    '#!/bin/sh' \
    'set -eu' \
    '' \
    '# Compose runs the app with working_dir=/data. The dashboard serves "web/dist"' \
    '# relative to cwd, so link it to bundled assets if the mount is empty.' \
    'if [ ! -e /data/web/dist ]; then' \
    '    mkdir -p /data/web' \
    '    ln -s /app/web/dist /data/web/dist' \
    'fi' \
    '' \
    'exec /usr/local/bin/meshenger "$@"' \
    > /usr/local/bin/docker-entrypoint.sh \
    && chmod +x /usr/local/bin/docker-entrypoint.sh

USER meshenger:meshenger

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["/config/config.toml"]
