use clap::Parser;
use log::info;
use std::thread;
use tiny_http::{Method, Response, Server};
use url::Url;

use conbee2_exporter::{metrics, run};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Conbee2 API server url
    #[clap(long, parse(try_from_str = Url::parse))]
    url: Url,

    /// Conbee2 API username
    #[clap(long)]
    username: String,

    /// Port to listen for metrics
    #[clap(short, long, default_value_t = 8000)]
    port: u16,
}

fn main() {
    let args = Args::parse();
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    info!("🚀 Starting conbee2-exporter");

    thread::spawn(move || {
        run(&args.url, &args.username).unwrap();
    });

    let server = Server::http(format!("0.0.0.0:{}", args.port)).unwrap();
    for request in server.incoming_requests() {
        match (request.method(), request.url()) {
            (Method::Get, "/metrics") => {
                let _ = request.respond(Response::from_string(metrics()));
            }
            _ => {
                let _ = request.respond(Response::from_string("Did you mean GET /metrics?\n"));
            }
        };
    }
}
