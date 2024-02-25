FROM rust:slim-bookworm

RUN apt-get update && apt-get -y install libssl-dev pkg-config

RUN mkdir /app
WORKDIR /app
ADD . .

RUN cargo build

ENTRYPOINT [ "cargo", "run", "--" ]
CMD ["--url http://<gateway rest API>:4501", "--username 0E87CDA111", "--port 9199"]