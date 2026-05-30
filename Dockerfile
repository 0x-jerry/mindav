FROM rust:1-slim-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:stable-slim
COPY --from=builder /src/target/release/mindav /mindav/mindav

WORKDIR /mindav/

ENV RUST_LOG=info

ENTRYPOINT ["/mindav/mindav"]

EXPOSE 8080
