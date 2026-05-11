# Multi-stage build: chef caches deps so a code-only change doesn't
# rebuild the whole world. Final image is `gcr.io/distroless/cc` —
# minimal libc + the binary, no shell, ~25 MB.

FROM rust:1.83-bookworm AS chef
RUN cargo install cargo-chef --locked --version 0.1.71
WORKDIR /build

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY apps/web/Cargo.toml apps/web/Cargo.toml
COPY crates/ crates/
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p argos-web
COPY . .
RUN cargo build --release -p argos-web

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /build/target/release/argos-web /usr/local/bin/argos-web
ENV ARGOS_BIND=0.0.0.0:8743 \
    ARGOS_DATA_DIR=/var/lib/argos-web \
    ARGOS_DOCS_DIR=/srv/argos-docs \
    ARGOS_DOWNLOADS_BASE=https://github.com/argos-app/argos/releases/latest/download \
    RUST_LOG=argos_web=info,tower_http=info
EXPOSE 8743
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/argos-web"]
