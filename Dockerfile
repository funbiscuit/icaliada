FROM lukemathwalker/cargo-chef:latest-rust-1.73.0-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN \
  apt-get update && \
  apt-get install -y ca-certificates && \
  apt-get clean

ENV APP_SERVER_HOST=0.0.0.0
ENV APP_SERVER_PORT=8080

COPY config-default.yml config-default.yml
COPY --from=builder /app/target/release/icaliada icaliada

ENTRYPOINT ["/app/icaliada"]