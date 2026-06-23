use std::{net::SocketAddr, process::ExitCode};

use client::{App, load_config};
use winit::event_loop::EventLoop;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let mut config = load_config(None);

    if let Some(addr_arg) = std::env::args().nth(1) {
        match addr_arg.parse::<SocketAddr>() {
            Ok(addr) => {
                config.server.address = addr.ip().to_string();
                config.server.port = addr.port();
            }
            Err(_) => {
                eprintln!("Invalid server address: {addr_arg}");
                return ExitCode::FAILURE;
            }
        }
    }

    let address_str = format!("{}:{}", config.server.address, config.server.port);
    tracing::info!("Connecting to {address_str}...");

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(config);
    event_loop.run_app(&mut app).unwrap();

    ExitCode::SUCCESS
}
