FROM rust:1.98.1-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --release --locked -p exchange-api

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 exchange

COPY --from=builder /app/target/release/exchange-api /usr/local/bin/exchange-api

USER exchange
WORKDIR /home/exchange

ENV API_BIND=0.0.0.0:3000

EXPOSE 3000

ENTRYPOINT ["/usr/local/bin/exchange-api"]
