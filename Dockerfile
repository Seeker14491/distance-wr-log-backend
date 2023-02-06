FROM rust:bookworm as builder

RUN DEBIAN_FRONTEND=noninteractive apt-get update -y && \
    apt-get install -y --no-install-recommends protobuf-compiler && \
    # Cleanup
    rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build --release


FROM debian:bookworm

RUN mkdir /data
VOLUME /data

WORKDIR /app
COPY --from=builder /src/target/release/distance-wr-log-bot /src/target/release/distance-wr-log-manager ./

CMD ./distance-wr-log-manager