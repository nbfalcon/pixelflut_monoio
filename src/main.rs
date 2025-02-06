#![feature(let_chains)]
#![feature(async_closure)]
pub mod core;
pub mod frontend;
pub mod protocol;

use core::{config::Config, game::PixelflutGame, state::PixelflutThreadState};
use frontend::winit::winit_window_loop;
use futures::StreamExt;
use monoio::{
    join, net::{TcpListener, TcpStream}, FusionDriver, RuntimeBuilder
};
use protocol::tcp_pixelflut::{tcp_pixelflut_handler, PixelflutClient};
use std::{
    fmt::Display, io, net::{SocketAddr, ToSocketAddrs}, os::fd::{FromRawFd, IntoRawFd, RawFd}, thread
};

struct AcceptedClient {
    stream: RawFd,
}

struct ServerCtx {
    thread_spawners: Box<[async_channel::Sender<AcceptedClient>]>,
}

impl ServerCtx {
    /// Spawn a client on a random thread
    async fn spawn(&self, client: AcceptedClient) -> bool {
        let i = rand::random_range(0..self.thread_spawners.len());
        self.thread_spawners[i].send(client).await.is_ok()
    }
}

fn tcp_listener_stream(
    listen: TcpListener,
) -> impl futures::Stream<Item = (TcpStream, SocketAddr)> {
    futures::stream::unfold(listen, async |listen: TcpListener| {
        match listen.accept().await {
            Ok(res) => Some((res, listen)),
            Err(_) => None,
        }
    })
}

fn tcp_listeners<A: ToSocketAddrs + Display>(
    addr: A,
) -> impl futures::Stream<Item = (TcpStream, SocketAddr)> {
    let mut listeners = Vec::new();
    let mut listen_on_any = false;
    for addr in addr.to_socket_addrs().unwrap() {
        listen_on_any = true;
        println!("Listening on {addr}");
        let listen = TcpListener::bind(addr).expect(&format!("failed to bind {addr}"));
        let stream = Box::pin(tcp_listener_stream(listen));
        listeners.push(stream);
    }
    if !listen_on_any {
        panic!("'{addr}' did not resolve to any listen address")
    }

    futures::stream::select_all(listeners)
}

async fn tcp_listener<A: ToSocketAddrs + Display>(addr: A, server: ServerCtx) -> io::Result<()> {
    // FIXME: config.addresses
    let mut listen = tcp_listeners(addr);
    while let Some((socket, _addr)) = listen.next().await {
        let socket = socket.into_raw_fd();
        if !server.spawn(AcceptedClient { stream: socket }).await {
            break;
        }
    }
    Ok(())
}

async fn channel_spawner(
    channel: async_channel::Receiver<AcceptedClient>,
    worker: &'static PixelflutThreadState,
) {
    while let Ok(message) = channel.recv().await {
        let stream =
            TcpStream::from_std(unsafe { std::net::TcpStream::from_raw_fd(message.stream) })
                .unwrap();
        monoio::spawn(tcp_pixelflut_handler(PixelflutClient::new(stream, worker)));
    }
}

async fn main_thread(
    channel: async_channel::Receiver<AcceptedClient>,
    worker: &'static PixelflutThreadState,
    config: Config,
    server: ServerCtx,
) {
    let (r1, _r2) = join!(
        monoio::spawn(tcp_listener(config.listen_addr, server)),
        monoio::spawn(channel_spawner(channel, worker)));
    r1.unwrap();
}

fn setup_server(config: Config) -> (&'static PixelflutGame, Vec<thread::JoinHandle<()>>) {
    assert!(config.num_io_threads >= 1);

    let game = PixelflutGame::new(&config);

    let mut thread_spawners = Vec::new();
    let mut thread_spawners_rx = Vec::new();
    thread_spawners.reserve(config.num_io_threads);
    thread_spawners_rx.reserve(config.num_io_threads);
    for _thread_id in 0..config.num_io_threads {
        let (tx, rx) = async_channel::bounded(128);
        thread_spawners.push(tx);
        thread_spawners_rx.push(rx);
    }
    let server = ServerCtx {
        thread_spawners: thread_spawners.into_boxed_slice(),
    };

    let mut join = Vec::new();
    // Spawn Main thread
    let main_receiver = thread_spawners_rx[0].clone();
    join.push(
        std::thread::Builder::new()
            .name(format!("IO Worker 0"))
            .spawn(move || {
                let mut runtime = RuntimeBuilder::<FusionDriver>::new()
                    .with_entries(256)
                    .build()
                    .expect("Failed to initialize runtime");

                runtime.block_on(main_thread(
                    main_receiver,
                    game.for_worker(0),
                    config,
                    server,
                ));
            })
            .expect("Spawn IO Thread"),
    );
    for (thread_id, spawner_channel_rx) in thread_spawners_rx.into_iter().enumerate().skip(1) {
        join.push(
            std::thread::Builder::new()
                .name(format!("IO Worker {thread_id}"))
                .spawn(move || {
                    let mut runtime = RuntimeBuilder::<FusionDriver>::new()
                        .with_entries(256)
                        .build()
                        .expect("Failed to initialize runtime");

                    runtime.block_on(channel_spawner(
                        spawner_channel_rx,
                        game.for_worker(thread_id),
                    ));
                })
                .expect("Spawn IO Thread")
        );
    }

    (game, join)
}

fn main() {
    // FIXME: read from TOML
    // FIXME: make num_threads configurable
    let config = Config {
        num_io_threads: 4,
        image_width: 1280,
        image_height: 720,
        listen_addr: "127.0.0.1:4000".to_owned(),
    };

    let (game, join) = setup_server(config.clone());

    winit_window_loop(&config, game);

    for join_h in join {
        join_h.join().unwrap();
    }
}
