// Re-export shared constants/helpers from gnils-protocol that are used
// directly in client source files.
pub use gnils_protocol::{
    WORLD_HALF_W, WORLD_HALF_H,
    PLAYER1_X, PLAYER2_X, PLAYER1_COLOR, PLAYER2_COLOR,
    PLAYER_Y_MIN, PLAYER_Y_MAX,
    MAX_FLIGHT, HIT_SCORE, SELF_HIT, PENALTY_FACTOR,
    is_on_screen, is_in_extended_range,
};

// ── Client-only constants ───────────────────────────────────────────────────

pub const MAX_POWER: f64 = 350.0;

pub const PARTICLE_5_MIN_SPEED: f64 = 100.0;
pub const PARTICLE_5_MAX_SPEED: f64 = 200.0;
pub const PARTICLE_10_MIN_SPEED: f64 = 150.0;
pub const PARTICLE_10_MAX_SPEED: f64 = 250.0;
pub const N_PARTICLES_5: u32 = 20;
pub const N_PARTICLES_10: u32 = 30;

pub const DEFAULT_MAX_PLANETS: u32 = 4;

pub const WINDOW_WIDTH: f32 = 800.0;
pub const WINDOW_HEIGHT: f32 = 600.0;

pub const SHIP_FRAME_WIDTH: u32 = 40;
pub const SHIP_FRAME_HEIGHT: u32 = 33;
