use clap::Parser;
use log::info;
use std::{panic, process, thread};
use tiny_http::{Method, Response, Server};
use url::Url;

use deconz_exporter::{metrics, run};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// deCONZ API server url
    #[clap(long, env = "DECONZ_API_URL", parse(try_from_str = Url::parse))]
    api_url: Url,

    /// Optional websocket URL override
    #[clap(long, env = "DECONZ_WS_URL")]
    ws_url: Option<String>,

    /// deCONZ API username
    #[clap(long, env = "DECONZ_API_USERNAME")]
    username: String,

    /// Port to listen for metrics
    #[clap(short, long, env = "DECONZ_PORT", default_value_t = 8000)]
    port: u16,
}

fn main() {
    let args = Args::parse();
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    info!("🚀 Starting deconz-exporter");

    // take_hook() returns the default hook in case when a custom one is not set
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        info!(
            "Something went terribly wrong and one of the threads panicked. Shutting down the main thread."
        );
        orig_hook(panic_info);
        process::exit(1);
    }));

    let ws_url = match args.ws_url.as_deref() {
        Some(value) if !value.is_empty() => match Url::parse(value) {
            Ok(url) => Some(url),
            Err(err) => {
                eprintln!("Invalid websocket URL in --ws-url/DECONZ_WS_URL: {}", err);
                process::exit(2);
            }
        },
        _ => None,
    };

    thread::spawn(move || {
        run(&args.api_url, ws_url.as_ref(), &args.username).unwrap();
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
