FROM rust:trixie AS builder

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

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
COPY config.example.toml /app/config.example.toml

RUN mkdir -p /config /data \
    && chown -R meshenger:meshenger /app /config /data /home/meshenger

USER meshenger:meshenger

ENTRYPOINT ["/usr/local/bin/meshenger"]
CMD ["/config/config.toml"]
