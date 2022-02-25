use conbee2_exporter::{metrics, process, Event};
use log::info;
use std::thread;

use tiny_http::{Method, Response, Server};

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    info!("🚀 Starting conbee2-exporter");

    thread::spawn(|| {
        let events = include_str!("../events.json");
        for event in events.lines().filter(|l| !l.trim().is_empty()) {
            let mut e = serde_json::from_str::<Event>(event)
                .unwrap_or_else(|err| panic!("Failed to parse event {}: {}", &event, err));

            process(&mut e)
                .unwrap_or_else(|err| panic!("Failed to process event {:?}: {}", &e, err));
        }
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
