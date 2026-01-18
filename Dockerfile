# Build stage
FROM rust:slim AS builder

RUN apt update && apt install -y libssl-dev pkg-config && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt update && apt install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/deconz-exporter /usr/local/bin/deconz-exporter

ENV DECONZ_API_URL=""
ENV DECONZ_API_USERNAME=""

ENTRYPOINT ["deconz-exporter"]
CMD ["--url", "${DECONZ_API_URL}", "--username", "${DECONZ_API_USERNAME}", "--port", "9199"]