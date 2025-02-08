use bbge::net::server::{deserialize_message, new_server, OutboundMessage, ServerFunctions};
use shared::{ClientMessage, GameState, Player, Vec2};
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc::UnboundedSender, RwLock};

fn main() {
    env_logger::init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(16)
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let game_state = Arc::new(RwLock::new(GameState::new()));
    let game_state_clone = Arc::clone(&game_state);
    let game_state_clone2 = Arc::clone(&game_state);
    let game_state_clone3 = Arc::clone(&game_state);

    let net_funcs = ServerFunctions {
        client_connected: move |client_id, _msg_sender| {
            let game_state = Arc::clone(&game_state_clone3);
            async move {
                client_connected(client_id, game_state, _msg_sender).await;
            }
        },
        client_disconnected: move |client_id, reason, _msg_sender| {
            let game_state = Arc::clone(&game_state_clone);
            async move {
                client_disconnected(client_id, reason, game_state).await;
            }
        },
        received_message: move |client_id, msg, msg_sender| {
            let msg: ClientMessage = deserialize_message(msg).unwrap();
            let game_state = Arc::clone(&game_state_clone2);

            async move {
                received_message(client_id, msg, game_state, msg_sender).await;
            }
        },
    };
    let server = new_server(rt.handle(), net_funcs).unwrap();

    let mut timer = tokio::time::interval(Duration::from_millis(10));
    rt.block_on(async move {
        loop {
            timer.tick().await;

            let game_state: GameState = { game_state.read().await.clone() };

            for conn_id in game_state.player_clients.keys() {
                if let Err(err) = server.send_unreliable_message(*conn_id, &game_state) {
                    eprintln!("failed to send message: {}", err);
                }
            }
        }
    })
}

async fn client_connected(
    client_id: u64,
    _game_state: Arc<RwLock<GameState>>,
    _outbound_msg_send_chan: UnboundedSender<OutboundMessage>,
) {
    log::info!("client connected: {}", client_id);
}

async fn received_message(
    client_id: u64,
    msg: ClientMessage,
    game_state: Arc<RwLock<GameState>>,
    _outbound_msg_send_chan: UnboundedSender<OutboundMessage>,
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

async fn client_disconnected(client_id: u64, reason: String, game_state: Arc<RwLock<GameState>>) {
    log::info!("client disconnected: {} ({})", client_id, reason);

    let game_state = &mut game_state.write().await;
    let player_id = game_state.player_clients.remove(&client_id).unwrap();
    game_state.remove_player(player_id);
}
