From  rust:1.62.1 as builder

WORKDIR /usr/src/server
COPY . .

RUN cargo install --path .

FROM debian:buster-slim
COPY --from=builder /usr/local/cargo/bin/main /usr/local/bin/main
CMD ["main"]
