FROM rustlang/rust:nightly-slim

RUN apt update && apt install -y libssl-dev pkg-config && rm -rf /var/lib/apt/lists/*

RUN mkdir /app
WORKDIR /app
ADD . .

RUN cargo build

ENTRYPOINT [ "cargo", "run", "--"]
CMD ["--url http://<deconz-ip>:<port>", "--username <username-from-setup>", "--port 9199"]