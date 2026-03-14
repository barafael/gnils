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
    pub color_rgb: (u8, u8, u8),
    /// Distance from center to gun point
    pub gun_offset: f64,
    /// Explosion animation frame counter
    pub explosion_frame: u32,
}

impl Player {
    pub fn color(&self) -> Color {
        Color::srgb(
            self.color_rgb.0 as f32 / 255.0,
            self.color_rgb.1 as f32 / 255.0,
            self.color_rgb.2 as f32 / 255.0,
        )
    }
}

#[derive(Component)]
pub struct Planet {
    pub mass: f64,
    pub radius: f64,
    pub pos: Vec2,
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
}

/// Second sprite layer for ship frame blending (companion to Player entity).
#[derive(Component)]
pub struct ShipBlendSprite {
    pub player_id: u8,
}

/// Full-screen dim sprite shown behind the zoom minimap.
#[derive(Component)]
pub struct ZoomDimSprite;

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
pub struct UiRoundOverlay;

#[derive(Component)]
pub struct UiDimOverlay;

#[derive(Component)]
pub struct UiEndRoundMsg;

/// Root container for the settings menu overlay.
#[derive(Component)]
pub struct UiMenuOverlay;

/// Text node inside the settings menu overlay.
#[derive(Component)]
pub struct UiMenuText;
