FROM --platform=$BUILDPLATFORM rust:1.98.1-bookworm@sha256:9a73a5088750b4c95158ab26629c854c3d6fc4b173cb7bc8079ad252d8ed7bfa AS frontend

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN rustup target add wasm32-unknown-unknown \
    && cargo build --release --locked -p exchange-frontend --target wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.128 --locked \
    && wasm-bindgen --target web --out-name exchange_frontend --out-dir /app/frontend/pkg /app/target/wasm32-unknown-unknown/release/exchange-frontend.wasm \
    && cp crates/frontend/dist/pkg/bootstrap.js /app/frontend/pkg/bootstrap.js


FROM rust:1.98.1-bookworm@sha256:9a73a5088750b4c95158ab26629c854c3d6fc4b173cb7bc8079ad252d8ed7bfa AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --release --locked -p exchange-api

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171 AS runtime

RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 exchange

COPY --from=frontend /app/frontend /app/frontend
COPY --from=builder /app/target/release/exchange-api /usr/local/bin/exchange-api

USER 10001
WORKDIR /home/exchange

ENV FRONTEND_DIR=/app/frontend
ENV API_BIND=0.0.0.0:3000

EXPOSE 3000

ENTRYPOINT ["/usr/local/bin/exchange-api"]
