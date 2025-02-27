use std::{collections::HashMap, fmt::Display};

pub use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Player {
    pub pos: Vec2,
    pub id: PlayerID,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PlayerID(u64);

impl Display for PlayerID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl PlayerID {
    pub fn new() -> Self {
        PlayerID(rand::random())
    }
}

impl From<u64> for PlayerID {
    fn from(value: u64) -> Self {
        PlayerID(value)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GameState {
    pub players: HashMap<PlayerID, Player>,
    pub player_clients: HashMap<u64, PlayerID>,
}

impl GameState {
    pub fn new() -> Self {
        GameState {
            players: HashMap::new(),
            player_clients: HashMap::new(),
        }
    }

    pub fn add_player(&mut self, player: Player) {
        self.players.insert(player.id, player);
    }

    pub fn remove_player(&mut self, id: PlayerID) {
        self.players.remove(&id);
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum ClientMessage {
    Register { id: PlayerID },
    Input(Input),
}

#[derive(Serialize, Deserialize, Default, PartialEq, Debug)]
pub struct Input {
    pub angle: Option<f32>,
}
