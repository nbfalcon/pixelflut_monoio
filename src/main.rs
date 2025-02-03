#![feature(let_chains)]
pub mod core;
pub mod frontend;
pub mod protocol;

use core::{config::Config, game::PixelflutGame, state::PixelflutThreadState};
use frontend::winit::winit_window_loop;
use monoio::{net::TcpListener, RuntimeBuilder};
use protocol::tcp_pixelflut::{io_task, PixelflutClient};
use std::io;

async fn tcp_listener(config: Config, worker: &'static PixelflutThreadState) -> io::Result<()> {
    // FIXME: config.addresses
    let listen = TcpListener::bind("127.0.0.1:4000")?;
    loop {
        let (socket, _addr) = listen.accept().await?;
        monoio::spawn(io_task(PixelflutClient::new(socket, worker)));
    }
}

fn io_thread(my_thread: &'static PixelflutThreadState, config: Config) {
    let mut runtime = RuntimeBuilder::<monoio::FusionDriver>::new()
        .with_entries(256)
        .enable_timer()
        .build()
        .expect("Failed to initialize runtime");
    runtime
        .block_on(tcp_listener(config.clone(), my_thread))
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

    let game = PixelflutGame::new(&config);
    let io = std::thread::spawn({
        // TODO: There has to be a cleaner way
        let config2 = config.clone();
        let iostate = game.for_worker(0);
        move || {
            io_thread(iostate, config2);
        }
    });

    winit_window_loop(&config, game);

    io.join().unwrap();
}
