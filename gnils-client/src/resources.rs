use bevy::prelude::*;

use crate::constants::*;

// ── Game phases ────────────────────────────────────────────────────────────

#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum GamePhase {
    /// Splash / main menu shown on startup.
    #[default]
    MainMenu,
    /// Connecting to server (network mode only).
    Connecting,
    /// Connected; waiting for the server to send GameStart once both players are ready.
    WaitingForOpponent,
    /// Loading assets; entered after GameStart or local New Game.
    Loading,
    RoundSetup,
    Aiming,
    Firing,
    RoundOver,
    GameOver,
}

// ── Network mode ───────────────────────────────────────────────────────────

/// Whether this session is local hotseat or networked.
#[derive(Resource, Default, Clone, PartialEq, Eq, Debug)]
pub enum NetworkMode {
    #[default]
    Local,
    /// Connected to server; `player_id` is 1 or 2.
    Network { player_id: u8 },
}

impl NetworkMode {
    pub fn is_network(&self) -> bool {
        matches!(self, NetworkMode::Network { .. })
    }
    pub fn player_id(&self) -> Option<u8> {
        if let NetworkMode::Network { player_id } = self {
            Some(*player_id)
        } else {
            None
        }
    }
}

/// Server address and certificate hash for joining a game.
#[derive(Resource, Default)]
pub struct JoinAddress {
    pub text: String,
    /// Certificate hash for WASM clients (hex).
    pub cert_hash: String,
}

/// Lobby menu state.
#[derive(Resource, Default)]
pub struct LobbyMenu {
    pub selected: usize,
    pub screen: LobbyScreen,
    /// For host screen: cert hash received from spawned gnils-server.
    pub cert_hash: String,
    pub server_spawned: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LobbyScreen {
    #[default]
    Main,
    NetworkSub,
    Host,
    Join,
    Settings,
    Help,
}

// ── Game settings ──────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct GameSettings {
    pub max_planets: u32,
    pub max_blackholes: u32,
    pub bounce: bool,
    pub invisible: bool,
    pub fixed_power: bool,
    pub particles_enabled: bool,
    pub max_rounds: u32,
    pub max_flight: i32,
    pub fullscreen: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            max_planets: DEFAULT_MAX_PLANETS,
            max_blackholes: 0,
            bounce: false,
            invisible: false,
            fixed_power: false,
            particles_enabled: true,
            max_rounds: 3,
            max_flight: MAX_FLIGHT,
            fullscreen: false,
        }
    }
}

impl GameSettings {
    pub fn to_protocol(&self) -> gnils_protocol::GameSettingsData {
        gnils_protocol::GameSettingsData {
            max_planets: self.max_planets,
            max_blackholes: self.max_blackholes,
            bounce: self.bounce,
            invisible: self.invisible,
            fixed_power: self.fixed_power,
            particles_enabled: self.particles_enabled,
            max_rounds: self.max_rounds,
            max_flight: self.max_flight,
        }
    }

    pub fn apply_from_protocol(&mut self, data: &gnils_protocol::GameSettingsData) {
        self.max_planets = data.max_planets;
        self.max_blackholes = data.max_blackholes;
        self.bounce = data.bounce;
        self.invisible = data.invisible;
        self.fixed_power = data.fixed_power;
        self.particles_enabled = data.particles_enabled;
        self.max_rounds = data.max_rounds;
        self.max_flight = data.max_flight;
    }
}

/// State of the in-game settings menu (opened with Escape during play).
#[derive(Resource, Default)]
pub struct MenuOpen {
    pub open: bool,
    pub selected: usize,
}

// ── Turn state ─────────────────────────────────────────────────────────────

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

// ── Asset / rendering resources ────────────────────────────────────────────

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
    pub shot: Handle<Image>,
    pub explosion: Handle<Image>,
    pub explosion_10: Handle<Image>,
    pub explosion_5: Handle<Image>,
    pub planets: [Handle<Image>; 8],
}

/// Pre-allocated images that receive the per-frame blended ship sprite.
#[derive(Resource)]
pub struct BlendedShipImages {
    pub handles: [Handle<Image>; 2],
}

/// Result of the last round (for end-round message display).
#[derive(Resource, Default)]
pub struct RoundResult {
    pub hit_player: u8,
    pub shooter: u8,
    pub self_hit: bool,
    pub hit_score: i32,
    pub quick_bonus: i32,
    pub power_penalty: i32,
    pub total_score: i32,
    pub message: String,
}

/// Queued particle spawn requests.
#[derive(Resource, Default)]
pub struct ParticleSpawnQueue {
    pub requests: Vec<ParticleSpawnRequest>,
}

pub struct ParticleSpawnRequest {
    pub pos: Vec2,
    pub count: u32,
    pub size: u8,
}

/// Queued missile impacts.
#[derive(Resource, Default)]
pub struct MissileImpactQueue {
    pub impacts: Vec<MissileImpact>,
}

pub struct MissileImpact {
    pub pos: Vec2,
    pub hit_type: crate::events::HitType,
}
