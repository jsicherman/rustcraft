use std::{net::SocketAddr, process::ExitCode};

use client::{App, settings::load_config};
use winit::event_loop::EventLoop;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::DEBUG.into())
                .add_directive("naga=info".parse().unwrap())
                .add_directive("wgpu_core=info".parse().unwrap())
                .add_directive("wgpu_hal=info".parse().unwrap()),
        )
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
