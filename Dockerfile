FROM lukemathwalker/cargo-chef:latest-rust-slim-bullseye AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install protobuf-compiler -y
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim AS runtime
RUN mkdir /data
WORKDIR /app
COPY --from=builder /app/target/release/distance-wr-log-bot /app/target/release/distance-wr-log-manager ./
ENTRYPOINT ["./distance-wr-log-manager"]