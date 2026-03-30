// Re-export shared constants/helpers from gnils-protocol that are used
// directly in client source files.
pub use gnils_protocol::{
    HIT_SCORE, MAX_FLIGHT, PENALTY_FACTOR, PLAYER_Y_MAX, PLAYER_Y_MIN, PLAYER1_COLOR, PLAYER1_X,
    PLAYER2_COLOR, PLAYER2_X, SELF_HIT, WORLD_HALF_H, WORLD_HALF_W, is_in_extended_range,
    is_on_screen,
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
