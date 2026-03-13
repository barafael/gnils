use bevy::prelude::*;

#[derive(Component)]
pub struct Player {
    pub id: u8,
    pub angle: f64,
    pub rel_rot: f64,
    pub power: f64,
    pub score: i32,
    pub attempts: u32,
    pub shot: bool,
    pub color: Color,
    pub color_rgb: (u8, u8, u8),
    /// Distance from center to gun point
    pub gun_offset: f64,
    /// Explosion animation frame counter
    pub explosion_frame: u32,
}

#[derive(Component)]
pub struct Planet {
    pub mass: f64,
    pub radius: f64,
    pub pos: Vec2,
    pub planet_n: u8,
    pub is_blackhole: bool,
}

#[derive(Component)]
pub struct GravityBody {
    pub pos: (f64, f64),
    pub velocity: (f64, f64),
    pub last_pos: (f64, f64),
    pub flight: i32,
}

#[derive(Component)]
pub struct MissileMarker {
    pub trail_color: (u8, u8, u8),
    pub power_penalty: i32,
    pub active: bool,
}

#[derive(Component)]
pub struct ParticleMarker {
    pub size: u8,
    pub impact_pos: (f64, f64),
}

#[derive(Component)]
pub struct AimLine;

#[derive(Component)]
pub struct TrailSprite;

#[derive(Component)]
pub struct UiScoreP1;

#[derive(Component)]
pub struct UiScoreP2;

#[derive(Component)]
pub struct UiAnglePower;

#[derive(Component)]
pub struct UiRoundInfo;

#[derive(Component)]
pub struct UiMissileStatus;

#[derive(Component)]
pub struct RoundOverlay;

#[derive(Component)]
pub struct DimOverlay;

#[derive(Component)]
pub struct BounceBorder;
