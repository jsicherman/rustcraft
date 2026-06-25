use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    process::ExitCode,
    time::{Duration, Instant},
};

use client::settings::load_config;
use server::{GameServer, WorldGeneration};

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
    let port = config.server.port;

    let public_addr_str = format!("{}:{}", config.server.address, port);
    let Ok(public_addr) = public_addr_str.parse::<SocketAddr>() else {
        eprintln!("Invalid public address in config: {public_addr_str}");
        return ExitCode::FAILURE;
    };

    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);

    tracing::info!("Starting server on {bind_addr} (public: {public_addr})...");

    let mut server = match GameServer::new(
        bind_addr,
        public_addr,
        config.host.max_clients,
        WorldGeneration::new(config.world.generator, config.world.seed),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start server: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut last = Instant::now();
    let hz = Duration::from_secs_f64(1.0 / config.host.tps as f64);
    loop {
        let now = Instant::now();
        if let Err(e) = server.update(now - last) {
            tracing::error!("Server error: {e}");
        }
        last = now;
        std::thread::sleep(hz);
    }
}
