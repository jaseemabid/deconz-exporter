FROM rust:slim-bookworm

RUN apt update && apt install -y libssl-dev pkg-config && rm -rf /var/lib/apt/lists/*

RUN mkdir /app
WORKDIR /app
ADD . .

RUN cargo build

ENTRYPOINT [ "cargo", "run", "--" ]
CMD ["--url http://<gateway rest API>:4501", "--username 0E87CDA111", "--port 9199"]