#![feature(let_chains)]
pub mod core;
pub mod frontend;
pub mod protocol;

use core::{config::Config, game::PixelflutGame, state::PixelflutIOWorkerState};
use monoio::{net::TcpListener, RuntimeBuilder};
use protocol::tcp_pixelflut::{io_task, PixelflutClient};
use std::io;

async fn tcp_listener(config: Config, worker: &'static PixelflutIOWorkerState) -> io::Result<()> {
    // FIXME: config.addresses
    let listen = TcpListener::bind("127.0.0.1:4000")?;

    loop {
        let (socket, _addr) = listen.accept().await?;
        monoio::spawn(io_task(PixelflutClient::new(socket, worker)));
    }
}

fn io_thread(thread: usize, config: &Config, game: &'static PixelflutGame) {
    let mut runtime = RuntimeBuilder::<monoio::FusionDriver>::new()
        .with_entries(65536)
        .enable_timer()
        .build()
        .expect("Failed to initialize runtime");
    runtime
        .block_on(tcp_listener(config.clone(), game.for_worker(0)))
        .expect("Failed to spawn listener");
}

fn main() {
    // FIXME: read from TOML
    // FIXME: make num_threads configurable
    let config = Config {
        num_io_threads: 1,
        image_width: 1280,
        image_height: 720,
    };

    let io = std::thread::spawn(move || {
        let game = PixelflutGame::new(&config);
        io_thread(0, &config, game);
    });
    io.join().unwrap();
}
