# The introducer on Fly.io: build the server, then run it in a small image. `fly deploy` from this folder.
FROM rust:1-bookworm AS build
WORKDIR /introducer
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
COPY --from=build /introducer/target/release/introducer /usr/local/bin/introducer
ENV PORT=3536
EXPOSE 3536
CMD ["introducer"]
