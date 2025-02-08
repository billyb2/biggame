use std::{
    future::Future, net::{SocketAddr, UdpSocket}, sync::Arc, time::{Duration, SystemTime}
};

use log::debug;
use renet2::{Bytes, ConnectionConfig, DefaultChannel, RenetServer, ServerEvent};
use renet2_netcode::{
    BoxedSocket, NativeSocket, NetcodeServerTransport, ServerCertHash, ServerSetupConfig,
    ServerSocket, WebServerDestination, WebSocketServer, WebSocketServerConfig, WebTransportServer,
    WebTransportServerConfig,
};
use renetcode2::ServerAuthentication;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{sync::{mpsc::{UnboundedReceiver, UnboundedSender}, Mutex}, time::Instant};
use warp::Filter;

struct ClientConnectionInfo {
    native_addr: String,
    wt_dest: WebServerDestination,
    ws_url: url::Url,
    cert_hash: ServerCertHash,
}

pub type MessageSender = UnboundedSender<OutboundMessage>;

pub struct ServerFunctions<C, D, R, CF, DF, RF>
where
    C: Fn(u64, MessageSender) -> CF + 'static,
    D: Fn(u64, String, MessageSender) -> DF + 'static,
    R: Fn(u64, Vec<u8>, MessageSender) -> RF + 'static,
    CF: Future<Output = ()> + Send + Sync,
    DF: Future<Output = ()> + Send + Sync,
    RF: Future<Output = ()> + Send + Sync,
{
    pub client_connected: C,
    pub client_disconnected: D,
    pub received_message: R,
}

pub struct OutboundMessage {
    pub client_id: Option<u64>,
    pub message: Bytes,
}

pub struct Server {
    outbound_msg_send_chan: UnboundedSender<OutboundMessage>,
}

impl Server{
    pub fn send_unreliable_message<M: Serialize>(&self, client_id: u64, message: &M) -> anyhow::Result<()> {
        let msg_bin = rmp_serde::to_vec(message)?;
        self.outbound_msg_send_chan.send(OutboundMessage {
            client_id: Some(client_id),
            message: Bytes::from(msg_bin),
        })?;

        Ok(())
    }

    pub fn broadcast_message<M: Serialize>(&self, message: &M) -> anyhow::Result<()> {
        let msg_bin = rmp_serde::to_vec(message)?;
        self.outbound_msg_send_chan.send(OutboundMessage {
            client_id: None,
            message: Bytes::from(msg_bin),
        })?;

        Ok(())
    }
}

pub fn new_server <
    C: Fn(u64, MessageSender) -> CF + Send,
    D: Fn(u64, String, MessageSender) -> DF + Send,
    R: Fn(u64, Vec<u8>, MessageSender) -> RF + Send,
    CF: Future<Output = ()> + Send + Sync + 'static,
    DF: Future<Output = ()> + Send + Sync + 'static,
    RF: Future<Output = ()> + Send + Sync + 'static,
>(rt: &tokio::runtime::Handle, net_funcs: ServerFunctions<C, D, R, CF, DF, RF>) -> anyhow::Result<Server> {
    let _guard = rt.enter();

    let http_addr: SocketAddr = "127.0.0.1:4433".parse()?;
    let max_clients = 1024;


    // Native socket
    let wildcard_addr: SocketAddr = "127.0.0.1:0".parse()?;
    let native_socket = NativeSocket::new(UdpSocket::bind(wildcard_addr)?)?;

    // WebTransport socket
    let (wt_socket, cert_hash) = {
        let (config, cert_hash) =
            WebTransportServerConfig::new_selfsigned(wildcard_addr, max_clients)?;
        (
            WebTransportServer::new(config, tokio::runtime::Handle::current())?,
            cert_hash,
        )
    };

    // WebSocket socket
    let ws_socket = {
        let config = WebSocketServerConfig::new(wildcard_addr, max_clients);
        WebSocketServer::new(config, tokio::runtime::Handle::current())?
    };

    // Save connection info
    let client_connection_info = ClientConnectionInfo {
        native_addr: native_socket.addr()?.to_string(),
        wt_dest: wt_socket.addr()?.into(),
        ws_url: ws_socket.url(),
        cert_hash,
    };

    // Setup netcode server transport
    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        ?;
    let server_config = ServerSetupConfig {
        current_time,
        max_clients,
        protocol_id: 0,
        socket_addresses: vec![
            vec![native_socket.addr()?],
            vec![wt_socket.addr()?],
            vec![ws_socket.addr()?],
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
    ?;
    debug!("transport created");

    let (outbound_msg_send_chan, msg_to_send_chan) = tokio::sync::mpsc::unbounded_channel();

    // Run HTTP server for clients to get connection info.
    rt.spawn(async move { run_http_server(http_addr, client_connection_info).await });

    let outbound_msg_send_chan_clone = outbound_msg_send_chan.clone();
    // the actual game server
    rt.spawn(async move {
        run_renet_server(
            transport,
            net_funcs,
            msg_to_send_chan,
            outbound_msg_send_chan_clone,
        )
        .await
    });

    Ok(Server {
        outbound_msg_send_chan,
    })
}

pub fn deserialize_message<D: DeserializeOwned>(msg: Vec<u8>) -> anyhow::Result<D> {
    Ok(rmp_serde::from_slice(&msg)?)
}


async fn run_renet_server<
    C: Fn(u64, MessageSender) -> CF,
    D: Fn(u64, String, MessageSender) -> DF,
    R: Fn(u64, Vec<u8>, MessageSender) -> RF,
    CF: Future<Output = ()> + Send + Sync + 'static,
    DF: Future<Output = ()> + Send + Sync + 'static,
    RF: Future<Output = ()> + Send + Sync + 'static,
>(
    mut transport: NetcodeServerTransport,
    net_funcs: ServerFunctions<C, D, R, CF, DF, RF>,
    mut messages_to_send: UnboundedReceiver<OutboundMessage>,
    outbound_msg_send_chan: UnboundedSender<OutboundMessage>,
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
                    let outbound_msg_send_chan = outbound_msg_send_chan.clone();
                    tokio::task::spawn((net_funcs.client_connected)(client_id, outbound_msg_send_chan));
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    let outbound_msg_send_chan = outbound_msg_send_chan.clone();
                    tokio::task::spawn((net_funcs.client_disconnected)(
                        client_id,
                        reason.to_string(),
                        outbound_msg_send_chan,
                    ));
                }
            }
        }

        for client_id in server.clients_id() {
            while let Some(message) = server.receive_message(client_id, DefaultChannel::Unreliable)
            {
                let outbound_msg_send_chan = outbound_msg_send_chan.clone();
                tokio::task::spawn((net_funcs.received_message)(
                    client_id,
                    message.to_vec(),
                    outbound_msg_send_chan,
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
