use glam::Vec2;
use shared::{GameState, Input, Player, PlayerID};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

pub struct Conn {
    pub ws_write: SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>,
    pub player_id: PlayerID,
}

#[tokio::main]
async fn main() {
    let bind_addr = "0.0.0.0:9001";
    let server = TcpListener::bind(bind_addr).await.unwrap();

    let connections: Arc<Mutex<Vec<Conn>>> = Arc::new(Mutex::new(Vec::new()));
    let game_state = Arc::new(Mutex::new(GameState::new()));

    let connections_clone = Arc::clone(&connections);
    let game_state_clone = Arc::clone(&game_state);
    tokio::task::spawn(async move {
        while let Ok((stream, _)) = server.accept().await {
            let connections = Arc::clone(&connections_clone);
            let game_state = Arc::clone(&game_state_clone);
            tokio::task::spawn(async move {
                let ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (ws_write, mut ws_read) = ws_stream.split();

                let player_id = PlayerID::new();
                let conn = Conn {
                    ws_write,
                    player_id,
                };
                connections.lock().await.push(conn);
                println!("Added a new player to the game.");
                game_state.lock().await.add_player(Player {
                    id: player_id,
                    pos: Vec2::new(50.0, 50.0),
                });

                while let Some(Ok(msg)) = ws_read.next().await {
                    if msg.is_close() {
                        break;
                    }

                    if !(msg.is_binary() || msg.is_text()) {
                        continue;
                    }
                    if let Message::Binary(msg) = msg {
                        let input = Input::deserialize(&msg);
                        let mut game_state = game_state.lock().await;
                        let player = game_state
                            .players
                            .iter_mut()
                            .find(|p| p.id == player_id)
                            .unwrap();

                        if let Some(angle) = input.angle {
                            player.pos.x += angle.cos() * 5.0;
                            player.pos.y += angle.sin() * 5.0;
                        }
                    }
                }

                game_state.lock().await.remove_player(player_id);
            });
        }
    });

    loop {
        let mut game_state = game_state.lock().await;
        update_game_state(&mut game_state);

        let mut connections = connections.lock().await;
        let game_state_bin = game_state.serialize();

        for conn in connections.iter_mut() {
            let sock = &mut conn.ws_write;
            let game_state_bin = game_state_bin.clone();
            let _ = sock.send(Message::Binary(game_state_bin)).await;
        }

        std::mem::drop(game_state);
        std::mem::drop(connections);
        tokio::time::sleep(Duration::from_secs_f64(1.0 / 20.0)).await;
    }
}

fn update_game_state(game_state: &mut GameState) {
    for player in game_state.players.iter_mut() {}
}

async fn handle_message(msg: Vec<u8>) {}
