# Build ONLY from tools/build-introducer-context.sh output, not this source directory.
# The curated context contains introducer code and public frontend assets, no host gateway/config.
FROM rust:1.99.1-bookworm AS build
WORKDIR /source
COPY tools/introducer ./tools/introducer
COPY tools/crabbridge/frontend ./tools/crabbridge/frontend
COPY web/crabbridge ./web/crabbridge
RUN cargo build --release --locked --features crabbridge-frontend --manifest-path tools/introducer/Cargo.toml

FROM debian:bookworm-slim
COPY --from=build /source/tools/introducer/target/release/introducer /usr/local/bin/introducer
USER 65532:65532
ENV PORT=3536
EXPOSE 3536
CMD ["introducer"]
