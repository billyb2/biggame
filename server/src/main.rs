use shared::{ClientMessage, GameState, Player, Vec2};
use std::{
    future::Future,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    runtime,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        Mutex, RwLock,
    },
};
use warp::Filter;

use log::debug;
use renet2::{ConnectionConfig, DefaultChannel, RenetServer, ServerEvent};
use renet2_netcode::{
    BoxedSocket, NativeSocket, NetcodeServerTransport, ServerCertHash, ServerSetupConfig,
    ServerSocket, WebServerDestination, WebSocketServer, WebSocketServerConfig, WebTransportServer,
    WebTransportServerConfig,
};
use renetcode2::ServerAuthentication;

struct ClientConnectionInfo {
    native_addr: String,
    wt_dest: WebServerDestination,
    ws_url: url::Url,
    cert_hash: ServerCertHash,
}

fn main() {
    let rt = runtime::Builder::new_multi_thread()
        .worker_threads(16)
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let http_addr: SocketAddr = "127.0.0.1:4433".parse().unwrap();
    let max_clients = 10;

    // Native socket
    let wildcard_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let native_socket = NativeSocket::new(UdpSocket::bind(wildcard_addr).unwrap()).unwrap();

    // WebTransport socket
    let (wt_socket, cert_hash) = {
        let (config, cert_hash) =
            WebTransportServerConfig::new_selfsigned(wildcard_addr, max_clients);
        (
            WebTransportServer::new(config, runtime::Handle::current()).unwrap(),
            cert_hash,
        )
    };

    // WebSocket socket
    let ws_socket = {
        let config = WebSocketServerConfig::new(wildcard_addr, max_clients);
        WebSocketServer::new(config, runtime::Handle::current()).unwrap()
    };

    // Save connection info
    let client_connection_info = ClientConnectionInfo {
        native_addr: native_socket.addr().unwrap().to_string(),
        wt_dest: wt_socket.addr().unwrap().into(),
        ws_url: ws_socket.url(),
        cert_hash,
    };

    // Setup netcode server transport
    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let server_config = ServerSetupConfig {
        current_time,
        max_clients,
        protocol_id: 0,
        socket_addresses: vec![
            vec![native_socket.addr().unwrap()],
            vec![wt_socket.addr().unwrap()],
            vec![ws_socket.addr().unwrap()],
        ],
        authentication: ServerAuthentication::Unsecure,
    };
    let transport = NetcodeServerTransport::new_with_sockets(
        server_config,
        Vec::from([
            BoxedSocket::new(native_socket),
            BoxedSocket::new(wt_socket),
            BoxedSocket::new(ws_socket),
        ]),
    )
    .unwrap();
    debug!("transport created");

    let game_state = Arc::new(RwLock::new(GameState::new()));

    // Run HTTP server for clients to get connection info.
    rt.spawn(async move { run_http_server(http_addr, client_connection_info).await });

    let (msg_sender, msg_chan_rcv) = tokio::sync::mpsc::unbounded_channel();

    let game_state_clone = Arc::clone(&game_state);
    let msg_sender_clone = msg_sender.clone();

    rt.spawn(async move {
        let game_state = game_state_clone;
        let game_state_clone = Arc::clone(&game_state);
        let msg_sender = msg_sender_clone;

        run_renet_server(
            transport,
            RenetFunctions {
                client_connected: |client_id| async move {
                    println!("client connected: {}", client_id);
                },
                client_disconnected: move |client_id, reason| {
                    let game_state = Arc::clone(&game_state_clone);
                    async move {
                        println!("client disconnected: {} ({})", client_id, reason);
                        client_disconnected(client_id, game_state).await;
                    }
                },
                received_message: move |client_id, message| {
                    let msg = ClientMessage::deserialize(&message);
                    let game_state = Arc::clone(&game_state);
                    let msg_sender = msg_sender.clone();

                    async move {
                        received_message(client_id, msg, game_state, msg_sender).await;
                    }
                },
            },
            msg_chan_rcv,
        )
        .await
    });

    let mut timer = tokio::time::interval(Duration::from_millis(10));
    rt.block_on(async move {
        loop {
            timer.tick().await;

            let (game_state, game_state_bin): (GameState, Vec<u8>) = {
                let game_state = game_state.read().await.clone();
                let game_state_bin = game_state.serialize();
                (game_state, game_state_bin)
            };

            for conn_id in game_state.player_clients.keys() {
                if let Err(err) = msg_sender.send(OutboundMessage {
                    client_id: Some(*conn_id),
                    message: game_state_bin.clone(),
                }) {
                    eprintln!("failed to send message: {}", err);
                }
            }
        }
    })
}

async fn received_message(
    client_id: u64,
    msg: ClientMessage,
    game_state: Arc<RwLock<GameState>>,
    msg_sender: UnboundedSender<OutboundMessage>,
) {
    match msg {
        ClientMessage::Register { id } => {
            println!("received register: {}", id);
            let game_state = &mut game_state.write().await;
            game_state.add_player(Player {
                pos: Vec2::new(250.0, 250.0),
                id,
            });
            game_state.player_clients.insert(client_id, id);
        }
        ClientMessage::Input(input) => {
            let mut game_state = game_state.write().await;
            let player_id = game_state.player_clients[&client_id];
            if let Some(player) = game_state.players.get_mut(&player_id) {
                if let Some(angle) = input.angle {
                    let dir: Vec2 = Vec2::new(angle.cos(), angle.sin());
                    player.pos += dir * 10.0;
                }
            }
        }
    }
}

async fn client_disconnected(client_id: u64, game_state: Arc<RwLock<GameState>>) {
    let game_state = &mut game_state.write().await;
    let player_id = game_state.player_clients.remove(&client_id).unwrap();
    game_state.remove_player(player_id);
}

pub struct OutboundMessage {
    client_id: Option<u64>,
    message: Vec<u8>,
}

pub struct RenetFunctions<C, D, R, CF, DF, RF>
where
    C: Fn(u64) -> CF + 'static,
    D: Fn(u64, String) -> DF + 'static,
    R: Fn(u64, Vec<u8>) -> RF + 'static,
    CF: Future<Output = ()> + Send + Sync,
    DF: Future<Output = ()> + Send + Sync,
    RF: Future<Output = ()> + Send + Sync,
{
    pub client_connected: C,
    pub client_disconnected: D,
    pub received_message: R,
}

async fn run_renet_server<
    C: Fn(u64) -> CF,
    D: Fn(u64, String) -> DF,
    R: Fn(u64, Vec<u8>) -> RF,
    CF: Future<Output = ()> + Send + Sync + 'static,
    DF: Future<Output = ()> + Send + Sync + 'static,
    RF: Future<Output = ()> + Send + Sync + 'static,
>(
    mut transport: NetcodeServerTransport,
    renet_functions: RenetFunctions<C, D, R, CF, DF, RF>,
    mut messages_to_send: UnboundedReceiver<OutboundMessage>,
) {
    println!("running renet server");

    let mut last_updated = Instant::now();
    let server = Arc::new(Mutex::new(RenetServer::new(ConnectionConfig::test())));

    let server_clone = Arc::clone(&server);
    tokio::task::spawn(async move {
        loop {
            while let Some(msg) = messages_to_send.recv().await {
                let server = &mut server_clone.lock().await;
                match msg.client_id {
                    Some(client_id) => {
                        server.send_message(client_id, DefaultChannel::Unreliable, msg.message)
                    }
                    None => server.broadcast_message(DefaultChannel::Unreliable, msg.message),
                }
            }

            println!("looping in odd spot");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    loop {
        let now = Instant::now();
        let duration = now - last_updated;
        last_updated = now;

        let server = &mut server.lock().await;

        transport.update(duration, server).unwrap();

        while let Some(event) = server.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    tokio::task::spawn((renet_functions.client_connected)(client_id));
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    tokio::task::spawn((renet_functions.client_disconnected)(
                        client_id,
                        reason.to_string(),
                    ));
                }
            }
        }

        for client_id in server.clients_id() {
            while let Some(message) = server.receive_message(client_id, DefaultChannel::Unreliable)
            {
                tokio::task::spawn((renet_functions.received_message)(
                    client_id,
                    message.to_vec(),
                ));
            }
        }

        transport.send_packets(server);
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

async fn run_http_server(http_addr: SocketAddr, client_connection_info: ClientConnectionInfo) {
    let native_addr = client_connection_info.native_addr;
    let wt_dest = client_connection_info.wt_dest;
    let ws_url = client_connection_info.ws_url;
    let cert_hash = client_connection_info.cert_hash;

    let native = warp::path!("native").map(move || warp::reply::json(&native_addr));

    let cors = warp::cors().allow_any_origin();
    let wasm = warp::path!("wasm")
        .map(move || warp::reply::json(&(&wt_dest, &cert_hash, &ws_url)))
        .with(cors);

    let routes = warp::get().and(native.or(wasm));

    warp::serve(routes).run(http_addr).await;
}
