use bevy::prelude::*;

// ── Small view-layer helpers ───────────────────────────────────────────────

/// A [`Color`] from an 8-bit RGB triple, the form the original's palette uses.
pub fn rgb(c: (u8, u8, u8)) -> Color {
    Color::srgb_u8(c.0, c.1, c.2)
}

/// [`Visibility::Visible`] when `shown`, [`Visibility::Hidden`] otherwise.
pub fn visibility(shown: bool) -> Visibility {
    if shown {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}

/// Where a player's gun points at the start of a round, in radians CCW from
/// east: player 1 faces east, player 2 west.
pub fn initial_angle(id: u8) -> f64 {
    if id == 1 {
        0.0
    } else {
        std::f64::consts::PI
    }
}

// ── Components ─────────────────────────────────────────────────────────────

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
    pub explosion_progress: f64,
}

impl Player {
    pub fn color(&self) -> Color {
        rgb(self.color_rgb)
    }

    /// Point the gun at `angle` (radians CCW from east), keeping `rel_rot` —
    /// the rotation applied to the sprite — in step with it.
    pub fn aim(&mut self, angle: f64, power: f64) {
        self.angle = angle;
        self.power = power;
        self.rel_rot = angle - initial_angle(self.id);
    }
}

#[derive(Component)]
pub struct Planet {
    pub mass: f64,
    pub radius: f64,
    pub pos: Vec2,
    pub is_blackhole: bool,
}

impl Planet {
    /// Whether `pos` is inside this body: the event horizon for a blackhole,
    /// the disc for a planet.
    pub fn contains(&self, pos: (f64, f64)) -> bool {
        let d_sq = (pos.0 - self.pos.x as f64).powi(2) + (pos.1 - self.pos.y as f64).powi(2);
        if self.is_blackhole {
            d_sq <= self.mass
        } else {
            d_sq <= self.radius * self.radius
        }
    }
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

/// World backdrop sprite, stretched to cover the visible camera area.
#[derive(Component)]
pub struct Backdrop;

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

/// Network games: a line under the HUD saying whose turn it is.
#[derive(Component)]
pub struct UiTurnBanner;

#[derive(Component)]
pub struct UiRoundOverlay;

#[derive(Component)]
pub struct UiDimOverlay;

#[derive(Component)]
pub struct UiEndRoundMsg;

/// Settings menu overlay (the dimmed full-screen backdrop).
#[derive(Component)]
pub struct UiMenuOverlay;

/// Column inside the settings box; its children are the menu rows.
#[derive(Component)]
pub struct UiMenuColumn;

/// One row of the settings menu (label column + optional value).
#[derive(Component)]
pub struct UiMenuRow;
