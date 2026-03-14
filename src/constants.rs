// Re-export shared game constants from gnils-protocol so all client modules
// can continue to use `use crate::constants::*` without changes.
pub use gnils_protocol::{
    GRAVITY, TICK_HZ,
    WORLD_HALF_W, WORLD_HALF_H,
    PLAYER1_X, PLAYER2_X, PLAYER1_COLOR, PLAYER2_COLOR,
    PLAYER_Y_MIN, PLAYER_Y_MAX,
    SHIP_HALF_W, SHIP_HALF_H,
    MISSILE_SPEED_SCALE, SELF_HIT_GRACE_TICKS, MAX_FLIGHT,
    HIT_SCORE, SELF_HIT, QUICK_SCORE_1, QUICK_SCORE_2, QUICK_SCORE_3, PENALTY_FACTOR,
    PLANET_SHIP_DISTANCE, PLANET_EDGE_DISTANCE,
    PLANET_MASS_MIN, PLANET_MASS_MAX, PLANET_RADIUS_SCALE, PLANET_RADIUS_EXPONENT,
    BLACKHOLE_MASS_MIN, BLACKHOLE_MASS_MAX, PLANET_OVERLAP_SCALE, PLANET_OVERLAP_MASS_K,
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
