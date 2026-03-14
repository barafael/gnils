use serde::{Deserialize, Serialize};

// ── Constants ──────────────────────────────────────────────────────────────

pub const SERVER_PORT: u16 = 5888;
/// "gnils" encoded as a u64
pub const PROTOCOL_ID: u64 = 0x0000_676E_696C_73;
pub const PRIVATE_KEY: [u8; 32] = [0u8; 32];

/// Distance from ship center to gun barrel tip (pygame-space pixels).
/// Derived from the original: rect.right - rect.centerx + 2/3 for a 40px-wide sprite.
pub const GUN_OFFSET_P1: f64 = 22.0; // rect.right  - rect.centerx + 2 = 20 + 2
pub const GUN_OFFSET_P2: f64 = 23.0; // rect.centerx - rect.left   + 3 = 20 + 3

// ── Shared data types (no Bevy dependency) ─────────────────────────────────

/// Planet data as sent over the network and used in pure physics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanetData {
    pub mass: f64,
    pub radius: f64,
    /// Position in pygame-space (top-left origin, Y-down), 0..800 / 0..600
    pub pos: (f64, f64),
    pub is_blackhole: bool,
    /// Texture index 0..7 (ignored for blackholes)
    pub texture_index: u8,
}

/// Snapshot of a physics body — used both in protocol messages and as the
/// pure-Rust simulation state inside the server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BodySnapshot {
    pub pos: (f64, f64),
    pub vel: (f64, f64),
    pub last_pos: (f64, f64),
    pub flight: i32,
    pub active: bool,
}

/// All game settings synced from host → server → joining client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSettingsData {
    pub max_planets: u32,
    pub max_blackholes: u32,
    pub bounce: bool,
    pub invisible: bool,
    pub fixed_power: bool,
    pub particles_enabled: bool,
    pub max_rounds: u32,
    pub max_flight: i32,
}

impl Default for GameSettingsData {
    fn default() -> Self {
        Self {
            max_planets: 4,
            max_blackholes: 0,
            bounce: false,
            invisible: false,
            fixed_power: false,
            particles_enabled: true,
            max_rounds: 0,
            max_flight: 750,
        }
    }
}

/// Describes the outcome of a missile impact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HitResult {
    Planet,
    Blackhole,
    Ship {
        hit_player: u8,
        shooter: u8,
        self_hit: bool,
    },
    Timeout,
}

// ── Network messages ───────────────────────────────────────────────────────

/// Messages sent from client → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Client announces it has finished loading and is ready to play.
    Ready,
    /// Live aim preview while the active player is adjusting (unreliable OK).
    AimUpdate { angle: f64, power: f64 },
    /// The active player fires the missile (reliable).
    FireShot { angle: f64, power: f64 },
}

/// Messages sent from server → client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Sent to both clients once two players are connected.
    GameStart {
        your_player_id: u8,
        settings: GameSettingsData,
    },
    /// Sent at the start of each round; includes planet layout and spawn Y positions.
    RoundSetup {
        round: u32,
        active_player: u8,
        planets: Vec<PlanetData>,
        /// Y position in pygame-space for player 1 and player 2.
        player_y: [f64; 2],
    },
    /// Relay of the active player's aim to the waiting client (unreliable OK).
    OpponentAim { angle: f64, power: f64 },
    /// Per-physics-tick missile position update during flight.
    MissileUpdate {
        snapshot: BodySnapshot,
        trail_color: (u8, u8, u8),
    },
    /// Particle explosion events (spawned by server, relayed to clients).
    ParticleSpawn {
        pos: (f32, f32),
        count: u32,
        size: u8,
    },
    /// End of the current shot — includes scoring information.
    RoundResult {
        hit: HitResult,
        scores: [i32; 2],
        /// True if this result ends the entire game.
        game_over: bool,
    },
    /// Opponent disconnected mid-game.
    OpponentDisconnected,
}

// ── Pure physics functions (shared between server and client local-mode) ───

pub const GRAVITY: f64 = 120.0;
/// Physics tick rate used by both client (FixedUpdate) and server.
pub const TICK_HZ: f64 = 30.0;

/// Advance one physics tick: apply gravity from all planets, then move the body.
/// Mutates `pos`, `vel`, `last_pos`, and `flight` in-place.
pub fn step_gravity(
    pos: &mut (f64, f64),
    vel: &mut (f64, f64),
    last_pos: &mut (f64, f64),
    flight: &mut i32,
    planets: &[PlanetData],
) {
    *last_pos = *pos;
    *flight -= 1;

    for planet in planets {
        let dx = pos.0 - planet.pos.0;
        let dy = pos.1 - planet.pos.1;
        let d = dx * dx + dy * dy;
        if d < 1e-10 {
            vel.0 -= 10_000.0;
            vel.1 -= 10_000.0;
            continue;
        }
        let d_sqrt = d.sqrt();
        vel.0 -= (GRAVITY * planet.mass * dx) / (d * d_sqrt);
        vel.1 -= (GRAVITY * planet.mass * dy) / (d * d_sqrt);
    }

    pos.0 += vel.0;
    pos.1 += vel.1;
}

/// Circle-line intersection used for accurate planet impact position.
/// Returns the impact point along the line from `p1` to `p2` at the circle surface.
pub fn circle_line_intersect(
    center: (f64, f64),
    r: f64,
    p1: (f64, f64),
    p2: (f64, f64),
) -> (f64, f64) {
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    let a = dx * dx + dy * dy;
    let b = 2.0 * (dx * p1.0 - dx * center.0 + dy * p1.1 - dy * center.1);
    let c = -2.0 * center.0 * p1.0 - 2.0 * center.1 * p1.1
        + p1.0 * p1.0
        + p1.1 * p1.1
        + center.0 * center.0
        + center.1 * center.1
        - r * r;
    let disc = b * b - 4.0 * a * c;

    if disc < 0.0 {
        // Fallback: return a point well outside the extended play area (±1200/±900).
        return (2000.0, 1500.0);
    }

    let mut alpha = (-b + disc.sqrt()) / (2.0 * a);
    if alpha > 1.0 {
        alpha = (-b - disc.sqrt()) / (2.0 * a);
    }
    alpha -= 0.05;

    (p1.0 + alpha * dx, p1.1 + alpha * dy)
}
