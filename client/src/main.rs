use ewebsock::WsEvent;
use macroquad::{input::is_key_down, prelude::*};
use shared::{GameState, Input};

#[macroquad::main("BasicShapes")]
async fn main() {
    #[cfg(debug_assertions)]
    let conn_string = "ws://localhost:9001";
    #[cfg(not(debug_assertions))]
    let conn_string = "wss://shooter3-server.fly.dev";

    let (mut sender, receiver) = ewebsock::connect(
        conn_string,
        ewebsock::Options {
            max_incoming_frame_size: usize::MAX,
        },
    )
    .unwrap();

    let mut game_state = GameState::new();

    loop {
        let mut x_movement: f32 = 0.0;
        let mut y_movement: f32 = 0.0;

        if is_key_down(KeyCode::W) {
            y_movement += -1.0;
        }

        if is_key_down(KeyCode::A) {
            x_movement += -1.0;
        }

        if is_key_down(KeyCode::S) {
            y_movement += 1.0;
        }

        if is_key_down(KeyCode::D) {
            x_movement += 1.0;
        }

        let angle = match x_movement == 0.0 && y_movement == 0.0 {
            true => None,
            false => Some(y_movement.atan2(x_movement)),
        };

        let input = Input { angle };
        if input != Input::default() {
            let input_bin = input.serialize();
            sender.send(ewebsock::WsMessage::Binary(input_bin));
        }
        if let Some(WsEvent::Message(ewebsock::WsMessage::Binary(msg))) = receiver.try_recv() {
            game_state = GameState::deserialize(&msg);
        }

        clear_background(RED);

        for player in game_state.players.iter() {
            draw_circle(player.pos.x, player.pos.y, 10.0, BLUE);
        }

        next_frame().await
    }
}
