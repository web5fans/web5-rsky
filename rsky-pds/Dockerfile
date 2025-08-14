# Use the official Rust image.
# https://hub.docker.com/_/rust
FROM rust:latest AS builder

# Copy local code to the container image.
WORKDIR /usr/src/rsky
COPY . .
COPY config.toml /.cargo/

# Now copy the real source code and build the final binary
COPY rsky-pds/src rsky-pds/src
COPY rsky-pds/migrations rsky-pds/migrations
COPY rsky-pds/diesel.toml rsky-pds/diesel.toml

RUN cargo build --release --package rsky-pds

FROM rust:latest
WORKDIR /usr/src/rsky
COPY --from=builder /usr/src/rsky/target/release/rsky-pds rsky-pds
# Run the web service on container startup with the same environment variables
CMD ["sh", "-c", "./rsky-pds"]
