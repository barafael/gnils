pub const GRAVITY: f64 = 120.0;
pub const MAX_POWER: f64 = 350.0;
pub const PLANET_SHIP_DISTANCE: f64 = 75.0;
pub const PLANET_EDGE_DISTANCE: f64 = 50.0;

pub const PARTICLE_5_MIN_SPEED: f64 = 100.0;
pub const PARTICLE_5_MAX_SPEED: f64 = 200.0;
pub const PARTICLE_10_MIN_SPEED: f64 = 150.0;
pub const PARTICLE_10_MAX_SPEED: f64 = 250.0;
pub const N_PARTICLES_5: u32 = 20;
pub const N_PARTICLES_10: u32 = 30;

pub const MAX_FLIGHT: i32 = 750;
pub const DEFAULT_MAX_PLANETS: u32 = 4;

pub const HIT_SCORE: i32 = 1500;
pub const SELF_HIT: i32 = 2000;
pub const QUICK_SCORE_1: i32 = 500;
pub const QUICK_SCORE_2: i32 = 200;
pub const QUICK_SCORE_3: i32 = 100;
pub const PENALTY_FACTOR: f64 = 5.0;

pub const FPS: f64 = 30.0;

pub const WINDOW_WIDTH: f32 = 800.0;
pub const WINDOW_HEIGHT: f32 = 600.0;

pub const PLAYER1_COLOR: (u8, u8, u8) = (209, 170, 133);
pub const PLAYER2_COLOR: (u8, u8, u8) = (132, 152, 192);

pub const PLAYER_Y_MIN: f64 = 100.0;
pub const PLAYER_Y_MAX: f64 = 500.0;

pub const SHIP_FRAME_WIDTH: u32 = 40;
pub const SHIP_FRAME_HEIGHT: u32 = 33;

/// Check if a position is within the visible screen (pygame coords).
pub fn is_on_screen(pos: (f64, f64)) -> bool {
    pos.0 >= 0.0 && pos.0 <= 800.0 && pos.1 >= 0.0 && pos.1 <= 600.0
}

/// Check if a position is within extended range (for cleanup).
pub fn is_in_extended_range(pos: (f64, f64)) -> bool {
    pos.0 >= -800.0 && pos.0 <= 2400.0 && pos.1 >= -600.0 && pos.1 <= 1800.0
}
