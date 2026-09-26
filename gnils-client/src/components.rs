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
    /// An impact stopped the missile this tick, and the trail should still
    /// reach the point where it stopped. Only a blackhole swallows a shot
    /// without leaving that last mark.
    pub draw_last_segment: bool,
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

/// The second camera that renders the zoom minimap.
#[derive(Component)]
pub struct MinimapCamera;

/// The minimap's own copy of the shot. The real one is hidden the moment it
/// leaves the main view — which is exactly when the minimap appears — so the
/// minimap draws its own, and can size it to stay visible out there.
#[derive(Component)]
pub struct MinimapMissile;

#[derive(Component)]
pub struct TrailSprite;

/// One of the HUD's text lines. A single component with a slot, rather than
/// a marker type per line, so the systems that write them need one query
/// instead of several that must be proved disjoint.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum HudSlot {
    /// Player 1's score, top left.
    ScoreP1,
    /// Player 2's score, top right.
    ScoreP2,
    /// The firing angle, top centre.
    Angle,
    /// The firing power, top centre beside the angle.
    Power,
    /// The round counter, bottom centre — replaced by `Timeout` in flight.
    RoundInfo,
    /// The power penalty of the shot in flight, top centre.
    PowerPenalty,
    /// How long the shot in flight has left, bottom centre.
    Timeout,
    /// Network games: whose turn it is.
    TurnBanner,
}

#[derive(Component)]
pub struct UiRoundOverlay;

/// The end-of-round panel's full-screen container.
#[derive(Component)]
pub struct UiEndRoundMsg;

/// The column inside the panel's box; its children are the rows.
#[derive(Component)]
pub struct UiEndRoundColumn;

/// One rebuilt row of the panel.
#[derive(Component)]
pub struct UiEndRoundRow;

/// The panel's last line. It carries a live countdown in network games, so
/// it is written in place rather than rebuilt with the rest.
#[derive(Component)]
pub struct UiEndRoundPrompt;

/// Settings menu overlay (the dimmed full-screen backdrop).
#[derive(Component)]
pub struct UiMenuOverlay;

/// Column inside the settings box; its children are the menu rows.
#[derive(Component)]
pub struct UiMenuColumn;

/// A node of the settings menu's body — a setting row, or the heading and
/// hint that frame them. Everything carrying this is despawned and rebuilt
/// together, so anything spawned into the menu without it would pile up.
#[derive(Component)]
pub struct UiMenuRow;
