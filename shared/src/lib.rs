use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Player {
    pub pos: Vec2,
    pub id: PlayerID,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PlayerID(u64);

impl PlayerID {
    pub fn new() -> Self {
        PlayerID(rand::random())
    }
}

#[derive(Serialize, Deserialize)]
pub struct GameState {
    pub players: Vec<Player>,
}

impl GameState {
    pub fn new() -> Self {
        GameState {
            players: Vec::new(),
        }
    }

    pub fn add_player(&mut self, player: Player) {
        self.players.push(player);
    }

    pub fn remove_player(&mut self, id: PlayerID) {
        self.players.retain(|player| player.id != id);
    }

    pub fn serialize(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap()
    }

    pub fn deserialize(data: &[u8]) -> Self {
        rmp_serde::from_slice(data).unwrap()
    }
}

#[derive(Serialize, Deserialize, Default, PartialEq, Debug)]
pub struct Input {
    pub angle: Option<f32>,
}

impl Input {
    pub fn serialize(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap()
    }

    pub fn deserialize(data: &[u8]) -> Self {
        rmp_serde::from_slice(data).unwrap()
    }
}
