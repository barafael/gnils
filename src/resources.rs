use bevy::prelude::*;

use crate::constants::*;

#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum GamePhase {
    #[default]
    Loading,
    RoundSetup,
    Aiming,
    Firing,
    RoundOver,
    GameOver,
}

#[derive(Resource)]
pub struct GameSettings {
    pub max_planets: u32,
    pub bounce: bool,
    pub invisible: bool,
    pub particles_enabled: bool,
    pub max_rounds: u32,
    pub max_flight: i32,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            max_planets: DEFAULT_MAX_PLANETS,
            bounce: false,
            invisible: false,
            particles_enabled: true,
            max_rounds: 0,
            max_flight: MAX_FLIGHT,
        }
    }
}

#[derive(Resource)]
pub struct TurnState {
    pub current_player: u8,
    pub last_player: u8,
    pub round: u32,
    pub round_over: bool,
    pub firing: bool,
    pub show_round: f64,
    pub show_planets: f64,
    pub game_over: bool,
}

impl TurnState {
    pub fn other_player(&self) -> u8 {
        3 - self.last_player
    }
}

impl Default for TurnState {
    fn default() -> Self {
        Self {
            current_player: 1,
            last_player: 1,
            round: 0,
            round_over: false,
            firing: false,
            show_round: 100.0,
            show_planets: 0.0,
            game_over: false,
        }
    }
}

#[derive(Resource)]
pub struct TrailCanvas {
    pub image_handle: Handle<Image>,
}

#[derive(Resource)]
pub struct BounceAnimation {
    pub count: f32,
    pub inc: f32,
}

impl Default for BounceAnimation {
    fn default() -> Self {
        Self {
            count: 255.0,
            inc: 7.0,
        }
    }
}

#[derive(Resource)]
pub struct GameAssets {
    pub font: Handle<Font>,
    pub backdrop: Handle<Image>,
    pub red_ship: Handle<Image>,
    pub blue_ship: Handle<Image>,
    pub ship_atlas_layout: Handle<TextureAtlasLayout>,
    pub shot: Handle<Image>,
    pub explosion_10: Handle<Image>,
    pub explosion_5: Handle<Image>,
    pub planets: [Handle<Image>; 8],
}

/// Queued particle spawn requests, processed each frame by the particle spawn system.
#[derive(Resource, Default)]
pub struct ParticleSpawnQueue {
    pub requests: Vec<ParticleSpawnRequest>,
}

pub struct ParticleSpawnRequest {
    pub pos: Vec2,
    pub count: u32,
    pub size: u8,
}

/// Queued missile impact, processed by the impact handling system.
#[derive(Resource, Default)]
pub struct MissileImpactQueue {
    pub impacts: Vec<MissileImpact>,
}

pub struct MissileImpact {
    pub pos: Vec2,
    pub hit_type: crate::events::HitType,
}
