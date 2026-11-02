FROM rust:1.99-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY crates crates
RUN cargo build --release --locked -p not-server

FROM debian:trixie-slim
RUN useradd --system --uid 10001 not && mkdir /data && chown not /data
COPY --from=build /src/target/release/not-server /usr/local/bin/not-server
USER not
VOLUME /data
EXPOSE 8080
ENV NOT_DB=/data/not.db NOT_ADDR=0.0.0.0:8080
ENTRYPOINT ["not-server"]
