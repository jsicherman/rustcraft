use std::{
    process::ExitCode,
    time::{Duration, Instant},
};

use client::{App, load_config};
use server::{DefaultWorldGenerator, GameServer, WorldGenerator};
use winit::event_loop::EventLoop;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let config = load_config(None);
    let address_str = format!("{}:{}", config.server.address, config.server.port);
    let Ok(address) = address_str.parse() else {
        eprintln!("Invalid server address: {address_str}");
        return ExitCode::FAILURE;
    };

    tracing::info!("Starting server...");

    std::thread::spawn(move || {
        let mut server = GameServer::new(
            address,
            address,
            config.host.max_clients,
            DefaultWorldGenerator::new(0),
        )
        .unwrap();

        let mut last = Instant::now();
        let hz = Duration::from_secs_f64(1.0 / config.host.tps as f64);
        loop {
            let now = Instant::now();
            server.update(now - last).unwrap();
            last = now;
            std::thread::sleep(hz);
        }
    });

    std::thread::sleep(Duration::from_millis(100));

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(config);
    event_loop.run_app(&mut app).unwrap();

    ExitCode::SUCCESS
}
