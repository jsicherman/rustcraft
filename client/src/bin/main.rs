use std::{
    process::ExitCode,
    time::{Duration, Instant},
};

use client::{App, settings::load_config};
use server::{GameServer, WorldGeneration};
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
            WorldGeneration::new(config.world.generator, config.world.seed),
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
