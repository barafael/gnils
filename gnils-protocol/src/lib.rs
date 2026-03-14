use rand::Rng;
use serde::{Deserialize, Serialize};

// ── Constants ──────────────────────────────────────────────────────────────

pub const SERVER_PORT: u16 = 5888;
/// "gnils" encoded as a u64
pub const PROTOCOL_ID: u64 = 0x0000_676E_696C_73;
pub const PRIVATE_KEY: [u8; 32] = [0u8; 32];

/// Distance from ship center to gun barrel tip (world units / pixels).
/// Derived from the original: rect.right - rect.centerx + 2/3 for a 40px-wide sprite.
pub const GUN_OFFSET_P1: f64 = 22.0; // rect.right  - rect.centerx + 2 = 20 + 2
pub const GUN_OFFSET_P2: f64 = 23.0; // rect.centerx - rect.left   + 3 = 20 + 3

// ── World / physics constants ───────────────────────────────────────────────

/// Bevy-space screen half-extents (center origin, Y-up).
pub const WORLD_HALF_W: f64 = 400.0;
pub const WORLD_HALF_H: f64 = 300.0;

/// Player spawn X positions (fixed, left and right of screen).
pub const PLAYER1_X: f64 = -360.0;
pub const PLAYER2_X: f64 = 360.0;

/// Player trail / UI colors (RGB).
pub const PLAYER1_COLOR: (u8, u8, u8) = (209, 170, 133);
pub const PLAYER2_COLOR: (u8, u8, u8) = (132, 152, 192);

/// Player Y spawn range (Bevy-space).
pub const PLAYER_Y_MIN: f64 = -200.0;
pub const PLAYER_Y_MAX: f64 = 200.0;

/// Ship collision bounding box half-extents (matches SHIP_FRAME_WIDTH/HEIGHT = 40x33).
pub const SHIP_HALF_W: f64 = 20.0;
pub const SHIP_HALF_H: f64 = 16.5;

/// Scale factor applied to power when computing initial missile velocity.
pub const MISSILE_SPEED_SCALE: f64 = 0.1;

/// Number of ticks after launch during which the shooter is immune to self-hit.
pub const SELF_HIT_GRACE_TICKS: i32 = 5;

/// Default maximum flight ticks before a missile times out.
pub const MAX_FLIGHT: i32 = 750;

pub const GRAVITY: f64 = 120.0;
/// Physics tick rate used by both client (FixedUpdate) and server.
pub const TICK_HZ: f64 = 30.0;

// ── Scoring constants ───────────────────────────────────────────────────────

pub const HIT_SCORE: i32 = 1500;
pub const SELF_HIT: i32 = 2000;
pub const QUICK_SCORE_1: i32 = 500;
pub const QUICK_SCORE_2: i32 = 200;
pub const QUICK_SCORE_3: i32 = 100;
/// Power penalty multiplier: penalty = -(PENALTY_FACTOR * power) as i32.
pub const PENALTY_FACTOR: f64 = 5.0;

// ── Planet generation constants ─────────────────────────────────────────────

/// Minimum clearance between a planet edge and the ship spawn columns.
pub const PLANET_SHIP_DISTANCE: f64 = 75.0;
/// Minimum clearance between a planet edge and the top/bottom screen edge.
pub const PLANET_EDGE_DISTANCE: f64 = 50.0;

pub const PLANET_MASS_MIN: f64 = 8.0;
pub const PLANET_MASS_MAX: f64 = 512.0;
/// radius = mass ^ PLANET_RADIUS_EXPONENT * PLANET_RADIUS_SCALE
pub const PLANET_RADIUS_SCALE: f64 = 12.5;
pub const PLANET_RADIUS_EXPONENT: f64 = 1.0 / 3.0;

pub const BLACKHOLE_MASS_MIN: f64 = 600.0;
pub const BLACKHOLE_MASS_MAX: f64 = 700.0;

/// Overlap check: distance must be >= (r1+r2)*SCALE + MASS_K*(m1+m2).
pub const PLANET_OVERLAP_SCALE: f64 = 1.5;
pub const PLANET_OVERLAP_MASS_K: f64 = 0.1;

// ── Shared data types (no Bevy dependency) ─────────────────────────────────

/// Planet data as sent over the network and used in pure physics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanetData {
    pub mass: f64,
    pub radius: f64,
    /// Position in Bevy-space (center origin, Y-up), -400..400 / -300..300
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
            max_flight: MAX_FLIGHT,
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
        /// Y position in Bevy-space for player 1 and player 2.
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

// ── Pure physics / game-logic functions ────────────────────────────────────

/// Check if a position is within the visible screen (Bevy coords, center origin, Y-up).
pub fn is_on_screen(pos: (f64, f64)) -> bool {
    pos.0 >= -WORLD_HALF_W && pos.0 <= WORLD_HALF_W
        && pos.1 >= -WORLD_HALF_H && pos.1 <= WORLD_HALF_H
}

/// Check if a position is within the extended play area (3× screen, for cleanup).
pub fn is_in_extended_range(pos: (f64, f64)) -> bool {
    pos.0 >= -3.0 * WORLD_HALF_W && pos.0 <= 3.0 * WORLD_HALF_W
        && pos.1 >= -3.0 * WORLD_HALF_H && pos.1 <= 3.0 * WORLD_HALF_H
}

/// Compute the gun barrel tip position given the ship center and aim angle.
/// `angle` is radians CCW from east (Bevy-native convention).
pub fn compute_launch_point(ship_x: f64, ship_y: f64, gun_offset: f64, angle: f64) -> (f64, f64) {
    (ship_x + gun_offset * angle.cos(), ship_y + gun_offset * angle.sin())
}

/// Compute initial missile velocity from power and aim angle.
/// `angle` is radians CCW from east (Bevy-native convention).
pub fn compute_launch_velocity(power: f64, angle: f64) -> (f64, f64) {
    (MISSILE_SPEED_SCALE * power * angle.cos(), MISSILE_SPEED_SCALE * power * angle.sin())
}

/// Reflect a missile off the screen boundaries in-place, interpolating the
/// perpendicular axis to the exact wall-crossing point.
pub fn apply_bounce(m: &mut BodySnapshot) {
    // Right wall
    if m.pos.0 > WORLD_HALF_W {
        let d = m.pos.0 - m.last_pos.0;
        if d.abs() > 1e-10 {
            m.pos.1 = m.last_pos.1
                + (m.pos.1 - m.last_pos.1) * (WORLD_HALF_W - m.last_pos.0) / d;
        }
        m.pos.0 = WORLD_HALF_W;
        m.vel.0 = -m.vel.0;
    }
    // Left wall
    if m.pos.0 < -WORLD_HALF_W {
        let d = m.last_pos.0 - m.pos.0;
        if d.abs() > 1e-10 {
            m.pos.1 = m.last_pos.1
                + (m.pos.1 - m.last_pos.1) * (m.last_pos.0 + WORLD_HALF_W) / d;
        }
        m.pos.0 = -WORLD_HALF_W;
        m.vel.0 = -m.vel.0;
    }
    // Top wall
    if m.pos.1 > WORLD_HALF_H {
        let d = m.pos.1 - m.last_pos.1;
        if d.abs() > 1e-10 {
            m.pos.0 = m.last_pos.0
                + (m.pos.0 - m.last_pos.0) * (WORLD_HALF_H - m.last_pos.1) / d;
        }
        m.pos.1 = WORLD_HALF_H;
        m.vel.1 = -m.vel.1;
    }
    // Bottom wall
    if m.pos.1 < -WORLD_HALF_H {
        let d = m.last_pos.1 - m.pos.1;
        if d.abs() > 1e-10 {
            m.pos.0 = m.last_pos.0
                + (m.pos.0 - m.last_pos.0) * (m.last_pos.1 + WORLD_HALF_H) / d;
        }
        m.pos.1 = -WORLD_HALF_H;
        m.vel.1 = -m.vel.1;
    }
}

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

/// Generate a randomised planet layout for one round.
/// Returns `Vec<PlanetData>` ready to be used for physics and/or Bevy spawning.
pub fn generate_planets(settings: &GameSettingsData, rng: &mut impl Rng) -> Vec<PlanetData> {
    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut out = Vec::new();

    if settings.max_blackholes > 0 {
        let n = rng.gen_range(1..=settings.max_blackholes);
        for _ in 0..n {
            for _ in 0..1000 {
                let mass   = rng.gen_range(BLACKHOLE_MASS_MIN..=BLACKHOLE_MASS_MAX);
                let radius = 1.0_f64;
                let margin = 3.0 * PLANET_SHIP_DISTANCE;
                let edge_m = 3.0 * PLANET_EDGE_DISTANCE;
                let x = rng.gen_range((-WORLD_HALF_W + margin + radius)..=(WORLD_HALF_W - margin - radius));
                let y = rng.gen_range((-WORLD_HALF_H + edge_m + radius)..=(WORLD_HALF_H - edge_m - radius));
                if planet_no_overlap(x, y, radius, mass, &placed) {
                    placed.push((x, y, radius, mass));
                    out.push(PlanetData { mass, radius, pos: (x, y), is_blackhole: true, texture_index: 0 });
                    break;
                }
            }
        }
    } else {
        let n = rng.gen_range(2..=settings.max_planets.max(2));
        let mut used: Vec<u8> = Vec::new();
        for _ in 0..n {
            for _ in 0..1000 {
                let mass   = rng.gen_range(PLANET_MASS_MIN..=PLANET_MASS_MAX);
                let radius = mass.powf(PLANET_RADIUS_EXPONENT) * PLANET_RADIUS_SCALE;
                let x = rng.gen_range((-WORLD_HALF_W + PLANET_SHIP_DISTANCE + radius)..=(WORLD_HALF_W - PLANET_SHIP_DISTANCE - radius));
                let y = rng.gen_range((-WORLD_HALF_H + PLANET_EDGE_DISTANCE + radius)..=(WORLD_HALF_H - PLANET_EDGE_DISTANCE - radius));
                if planet_no_overlap(x, y, radius, mass, &placed) {
                    let mut ti = rng.gen_range(0..8u8);
                    for _ in 0..20 { if !used.contains(&ti) { break; } ti = rng.gen_range(0..8u8); }
                    used.push(ti);
                    placed.push((x, y, radius, mass));
                    out.push(PlanetData { mass, radius, pos: (x, y), is_blackhole: false, texture_index: ti });
                    break;
                }
            }
        }
    }
    out
}

fn planet_no_overlap(x: f64, y: f64, r: f64, m: f64, placed: &[(f64, f64, f64, f64)]) -> bool {
    placed.iter().all(|&(px, py, pr, pm)| {
        ((x - px).powi(2) + (y - py).powi(2)).sqrt()
            >= (r + pr) * PLANET_OVERLAP_SCALE + PLANET_OVERLAP_MASS_K * (m + pm)
    })
}
