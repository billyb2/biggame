mod net;

use net::new_renet_client;
use renet2::DefaultChannel;
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use wasmtimer::std::{Instant, SystemTime};

use macroquad::{input::is_key_down, prelude::*};
use shared::{ClientMessage, GameState, Input, PlayerID};

#[macroquad::main("BasicShapes")]
async fn main() {
    #[cfg(target_family = "wasm")]
    {
        console_error_panic_hook::set_once();
        console_log::init_with_level(log::Level::Info).expect("error initializing logger");
    }

    let (mut client, mut transport) = new_renet_client();
    let mut last_updated = Instant::now();

    let player_id = PlayerID::new();
    let mut registered = false;
    let mut last_register_sent = Instant::now();

    let mut game_state = GameState::new();
    loop {
        // recv net
        while let Some(game_state_bin) = client.receive_message(0) {
            log::info!("received game state");
            registered = true;
            game_state = GameState::deserialize(&game_state_bin);
        }

        let now = Instant::now();
        let duration = now - last_updated;
        last_updated = now;

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
            let msg_bin = msg.serialize();
            client.send_message(DefaultChannel::Unreliable, msg_bin);
        }

        if client.is_connected() && !registered && last_register_sent.elapsed().as_secs_f32() > 1.0
        {
            last_register_sent = Instant::now();
            log::info!("sending register message");
            let msg = ClientMessage::Register { id: player_id };
            let msg_bin = msg.serialize();
            client.send_message(DefaultChannel::Unreliable, msg_bin);
        }

        client.update(duration);
        transport.update(duration, &mut client).unwrap();

        transport.send_packets(&mut client).unwrap();

        next_frame().await;
    }
}
