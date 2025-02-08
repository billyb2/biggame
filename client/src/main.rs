use bbge::net::client::{new_client, Instant};
use macroquad::{input::is_key_down, prelude::*};
use shared::{ClientMessage, GameState, Input, PlayerID};

#[macroquad::main("BasicShapes")]
async fn main() {
    #[cfg(target_family = "wasm")]
    {
        console_error_panic_hook::set_once();
        console_log::init_with_level(log::Level::Info).expect("error initializing logger");
    }

    let mut client = new_client("127.0.0.1".parse().unwrap()).unwrap();

    let player_id = PlayerID::new();
    let mut registered = false;

    let mut game_state = GameState::new();
    let mut last_register_sent = Instant::now();
    loop {
        // recv net
        while let Some(Ok(net_game_state)) = client.receive_unreliable() {
            log::info!("received game state");
            registered = true;
            game_state = net_game_state;
        }

        let mut input = Input::default();
        let mut x = 0.0f32;
        let mut y = 0.0f32;

        // Convert WASD keys to x/y components
        if is_key_down(KeyCode::W) {
            y -= 1.0;
        }
        if is_key_down(KeyCode::S) {
            y += 1.0;
        }
        if is_key_down(KeyCode::A) {
            x -= 1.0;
        }
        if is_key_down(KeyCode::D) {
            x += 1.0;
        }

        // Only set angle if there's actual input
        if x != 0.0 || y != 0.0 {
            // Convert x/y components to angle using atan2
            input.angle = Some(y.atan2(x));
        }

        // render
        clear_background(RED);
        game_state.players.values().for_each(|player| {
            draw_rectangle(player.pos.x, player.pos.y, 20.0, 20.0, BLUE);
        });

        //send net
        if registered && input.angle.is_some() {
            let msg = ClientMessage::Input(input);
            client.send_unreliable(&msg).unwrap();
        }

        if !registered && last_register_sent.elapsed().as_secs_f32() > 1.0 {
            last_register_sent = Instant::now();
            log::info!("sending register message");
            let msg = ClientMessage::Register { id: player_id };
            client.send_unreliable(&msg).unwrap();
        }

        client.tick().unwrap();

        next_frame().await;
    }
}
