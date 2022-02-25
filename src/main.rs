use conbee2_exporter::{metrics, run};
use log::info;
use std::thread;

use tiny_http::{Method, Response, Server};

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    info!("🚀 Starting conbee2-exporter");

    thread::spawn(|| {
        run("ws://nyx.jabid.in:4502").unwrap();
    });

    let server = Server::http("0.0.0.0:8000").unwrap();
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
