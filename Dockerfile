From  rust:latest as builder

WORKDIR /usr/src/server

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main(){}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/src/server/target \
    cargo build --release

COPY . .

RUN cargo install --path .


FROM debian:buster-slim
COPY --from=builder /usr/local/cargo/bin/tokio /usr/local/bin/main

EXPOSE 8080
