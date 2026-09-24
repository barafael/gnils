use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

// ── Constants ──────────────────────────────────────────────────────────────

/// Distance from ship center to gun barrel tip (world units / pixels).
pub const GUN_OFFSET_P1: f64 = 22.0;
pub const GUN_OFFSET_P2: f64 = 23.0;

// ── World / physics constants ───────────────────────────────────────────────

pub const WORLD_HALF_W: f64 = 400.0;
pub const WORLD_HALF_H: f64 = 300.0;

pub const PLAYER1_X: f64 = -360.0;
pub const PLAYER2_X: f64 = 360.0;

pub const PLAYER1_COLOR: (u8, u8, u8) = (209, 170, 133);
pub const PLAYER2_COLOR: (u8, u8, u8) = (132, 152, 192);

pub const PLAYER_Y_MIN: f64 = -200.0;
pub const PLAYER_Y_MAX: f64 = 200.0;

pub const SHIP_HALF_W: f64 = 20.0;
pub const SHIP_HALF_H: f64 = 16.5;

pub const MISSILE_SPEED_SCALE: f64 = 0.1;
pub const SELF_HIT_GRACE_TICKS: i32 = 5;
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
pub const PENALTY_FACTOR: f64 = 5.0;

// ── Planet generation constants ─────────────────────────────────────────────

pub const PLANET_SHIP_DISTANCE: f64 = 75.0;
pub const PLANET_EDGE_DISTANCE: f64 = 50.0;

pub const PLANET_MASS_MIN: f64 = 8.0;
pub const PLANET_MASS_MAX: f64 = 512.0;
pub const PLANET_RADIUS_SCALE: f64 = 12.5;
pub const PLANET_RADIUS_EXPONENT: f64 = 1.0 / 3.0;

pub const BLACKHOLE_MASS_MIN: f64 = 600.0;
pub const BLACKHOLE_MASS_MAX: f64 = 700.0;

pub const PLANET_OVERLAP_SCALE: f64 = 1.5;
pub const PLANET_OVERLAP_MASS_K: f64 = 0.1;

// ── Bounce boundaries ───────────────────────────────────────────────────────
// The original bounces at pixel-grid edges 0 and 799/599 (800x600 screen).
// In center-origin Y-up coords this is -400..399 / -300..299.

pub const BOUNCE_X_MIN: f64 = -WORLD_HALF_W; // -400 (left)
pub const BOUNCE_X_MAX: f64 = WORLD_HALF_W - 1.0; // 399 (right)
pub const BOUNCE_Y_MIN: f64 = -WORLD_HALF_H; // -300 (bottom)
pub const BOUNCE_Y_MAX: f64 = WORLD_HALF_H - 1.0; // 299 (top)

// ── Shared data types ──────────────────────────────────────────────────────

/// Planet data as sent over the network and used in pure physics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanetData {
    pub mass: f64,
    pub radius: f64,
    /// Position in Bevy-space (center origin, Y-up), -400..400 / -300..300
    pub pos: (f64, f64),
    pub is_blackhole: bool,
    /// Texture index 0..7 (ignored for blackholes)
    pub texture_index: u8,
}

/// Snapshot of a physics body — used in protocol messages and server simulation.
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
    pub random: bool,
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
            random: false,
        }
    }
}

// ── Pure physics / game-logic functions ────────────────────────────────────

pub fn is_on_screen(pos: (f64, f64)) -> bool {
    pos.0 >= -WORLD_HALF_W
        && pos.0 < WORLD_HALF_W
        && pos.1 >= -WORLD_HALF_H
        && pos.1 < WORLD_HALF_H
}

pub fn is_in_extended_range(pos: (f64, f64)) -> bool {
    pos.0 >= -3.0 * WORLD_HALF_W
        && pos.0 <= 3.0 * WORLD_HALF_W
        && pos.1 >= -3.0 * WORLD_HALF_H
        && pos.1 <= 3.0 * WORLD_HALF_H
}

pub fn compute_launch_point(ship_x: f64, ship_y: f64, gun_offset: f64, angle: f64) -> (f64, f64) {
    (
        ship_x + gun_offset * angle.cos(),
        ship_y + gun_offset * angle.sin(),
    )
}

pub fn compute_launch_velocity(power: f64, angle: f64) -> (f64, f64) {
    (
        MISSILE_SPEED_SCALE * power * angle.cos(),
        MISSILE_SPEED_SCALE * power * angle.sin(),
    )
}

pub fn apply_bounce(m: &mut BodySnapshot) {
    if m.pos.0 > BOUNCE_X_MAX {
        let d = m.pos.0 - m.last_pos.0;
        if d.abs() > 1e-10 {
            m.pos.1 = m.last_pos.1 + (m.pos.1 - m.last_pos.1) * (BOUNCE_X_MAX - m.last_pos.0) / d;
        }
        m.pos.0 = BOUNCE_X_MAX;
        m.vel.0 = -m.vel.0;
    }
    if m.pos.0 < BOUNCE_X_MIN {
        let d = m.last_pos.0 - m.pos.0;
        if d.abs() > 1e-10 {
            m.pos.1 = m.last_pos.1 + (m.pos.1 - m.last_pos.1) * (m.last_pos.0 - BOUNCE_X_MIN) / d;
        }
        m.pos.0 = BOUNCE_X_MIN;
        m.vel.0 = -m.vel.0;
    }
    if m.pos.1 > BOUNCE_Y_MAX {
        let d = m.pos.1 - m.last_pos.1;
        if d.abs() > 1e-10 {
            m.pos.0 = m.last_pos.0 + (m.pos.0 - m.last_pos.0) * (BOUNCE_Y_MAX - m.last_pos.1) / d;
        }
        m.pos.1 = BOUNCE_Y_MAX;
        m.vel.1 = -m.vel.1;
    }
    if m.pos.1 < BOUNCE_Y_MIN {
        let d = m.last_pos.1 - m.pos.1;
        if d.abs() > 1e-10 {
            m.pos.0 = m.last_pos.0 + (m.pos.0 - m.last_pos.0) * (m.last_pos.1 - BOUNCE_Y_MIN) / d;
        }
        m.pos.1 = BOUNCE_Y_MIN;
        m.vel.1 = -m.vel.1;
    }
}

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
        return (2000.0, 1500.0);
    }

    let mut alpha = (-b + disc.sqrt()) / (2.0 * a);
    if alpha > 1.0 {
        alpha = (-b - disc.sqrt()) / (2.0 * a);
    }
    alpha -= 0.05;

    (p1.0 + alpha * dx, p1.1 + alpha * dy)
}

pub fn compute_shot_score(
    self_hit: bool,
    power_penalty: i32,
    shooter_attempts: u32,
) -> (i32, i32, i32) {
    if self_hit {
        return (-SELF_HIT, 0, 0);
    }
    let quick_bonus = match shooter_attempts {
        1 => QUICK_SCORE_1,
        2 => QUICK_SCORE_2,
        3 => QUICK_SCORE_3,
        _ => 0,
    };
    (
        HIT_SCORE + power_penalty + quick_bonus,
        quick_bonus,
        power_penalty,
    )
}

/// Generate a randomised planet layout for one round.
pub fn generate_planets(
    settings: &GameSettingsData,
    rng: &mut impl Rng,
) -> Vec<PlanetData> {
    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut out = Vec::new();

    if settings.max_blackholes > 0 {
        let n = rng.random_range(1..=settings.max_blackholes);
        for _ in 0..n {
            for _ in 0..1000 {
                let mass = rng.random_range(BLACKHOLE_MASS_MIN..=BLACKHOLE_MASS_MAX);
                let radius = 1.0_f64;
                let margin = 3.0 * PLANET_SHIP_DISTANCE;
                let edge_m = 3.0 * PLANET_EDGE_DISTANCE;
                let x = rng.random_range(
                    (-WORLD_HALF_W + margin + radius)..=(WORLD_HALF_W - margin - radius),
                );
                let y = rng.random_range(
                    (-WORLD_HALF_H + edge_m + radius)..=(WORLD_HALF_H - edge_m - radius),
                );
                if planet_no_overlap(x, y, radius, mass, &placed) {
                    placed.push((x, y, radius, mass));
                    out.push(PlanetData {
                        mass,
                        radius,
                        pos: (x, y),
                        is_blackhole: true,
                        texture_index: 0,
                    });
                    break;
                }
            }
        }
    } else {
        let n = rng.random_range(2..=settings.max_planets.max(2));
        let mut used: Vec<u8> = Vec::new();
        for _ in 0..n {
            for _ in 0..1000 {
                let mass = rng.random_range(PLANET_MASS_MIN..=PLANET_MASS_MAX);
                let radius = mass.powf(PLANET_RADIUS_EXPONENT) * PLANET_RADIUS_SCALE;
                let x = rng.random_range(
                    (-WORLD_HALF_W + PLANET_SHIP_DISTANCE + radius)
                        ..=(WORLD_HALF_W - PLANET_SHIP_DISTANCE - radius),
                );
                let y = rng.random_range(
                    (-WORLD_HALF_H + PLANET_EDGE_DISTANCE + radius)
                        ..=(WORLD_HALF_H - PLANET_EDGE_DISTANCE - radius),
                );
                if planet_no_overlap(x, y, radius, mass, &placed) {
                    let mut ti = rng.random_range(0..8u8);
                    for _ in 0..20 {
                        if !used.contains(&ti) {
                            break;
                        }
                        ti = rng.random_range(0..8u8);
                    }
                    used.push(ti);
                    placed.push((x, y, radius, mass));
                    out.push(PlanetData {
                        mass,
                        radius,
                        pos: (x, y),
                        is_blackhole: false,
                        texture_index: ti,
                    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn is_on_screen_boundaries() {
        assert!(is_on_screen((0.0, 0.0)));
        // Inclusive lower/left edges
        assert!(is_on_screen((-WORLD_HALF_W, -WORLD_HALF_H)));
        // Exclusive upper/right edges
        assert!(!is_on_screen((WORLD_HALF_W, 0.0)));
        assert!(!is_on_screen((0.0, WORLD_HALF_H)));
        assert!(!is_on_screen((401.0, 0.0)));
    }

    #[test]
    fn is_in_extended_range_is_3x_world() {
        assert!(is_in_extended_range((0.0, 0.0)));
        assert!(is_in_extended_range((
            2.9 * WORLD_HALF_W,
            -2.9 * WORLD_HALF_H
        )));
        assert!(!is_in_extended_range((3.1 * WORLD_HALF_W, 0.0)));
        assert!(!is_in_extended_range((0.0, 3.1 * WORLD_HALF_H)));
    }

    #[test]
    fn launch_point_offsets_along_angle() {
        let (x, y) = compute_launch_point(0.0, 0.0, 10.0, 0.0);
        assert_eq!((x, y), (10.0, 0.0));
        let (x, y) = compute_launch_point(0.0, 0.0, 10.0, std::f64::consts::FRAC_PI_2);
        assert!(x.abs() < 1e-9 && (y - 10.0).abs() < 1e-9);
    }

    #[test]
    fn launch_velocity_scales_with_power() {
        let (vx, vy) = compute_launch_velocity(100.0, 0.0);
        assert!((vx - MISSILE_SPEED_SCALE * 100.0).abs() < 1e-9);
        assert!(vy.abs() < 1e-9);
    }

    #[test]
    fn shot_score_self_hit_is_negative() {
        let (total, quick, pen) = compute_shot_score(true, -50, 1);
        assert_eq!(total, -SELF_HIT);
        assert_eq!((quick, pen), (0, 0));
    }

    #[test]
    fn shot_score_quickhit_decays_with_attempts() {
        let (total1, bonus1, _) = compute_shot_score(false, 0, 1);
        let (total2, bonus2, _) = compute_shot_score(false, 0, 2);
        let (total3, bonus3, _) = compute_shot_score(false, 0, 3);
        let (total4, bonus4, _) = compute_shot_score(false, 0, 4);
        assert_eq!(bonus1, QUICK_SCORE_1);
        assert_eq!(bonus2, QUICK_SCORE_2);
        assert_eq!(bonus3, QUICK_SCORE_3);
        assert_eq!(bonus4, 0);
        assert!(total1 > total2 && total2 > total3 && total3 > total4);
    }

    #[test]
    fn shot_score_applies_power_penalty() {
        let (total, _, pen) = compute_shot_score(false, -123, 1);
        assert_eq!(pen, -123);
        assert_eq!(total, HIT_SCORE - 123 + QUICK_SCORE_1);
    }

    #[test]
    fn step_gravity_decrements_flight_and_moves() {
        let mut pos = (0.0, 0.0);
        let mut vel = (1.0, 0.0);
        let mut last = (99.0, 99.0);
        let mut flight = 10;
        step_gravity(&mut pos, &mut vel, &mut last, &mut flight, &[]);
        assert_eq!(flight, 9);
        assert_eq!(last, (0.0, 0.0));
        assert!((pos.0 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn step_gravity_is_deterministic() {
        let planets = vec![PlanetData {
            mass: 100.0,
            radius: 10.0,
            pos: (50.0, 20.0),
            is_blackhole: false,
            texture_index: 0,
        }];
        let mut a = ((0.0, 0.0), (2.0, 3.0), (0.0, 0.0), 100_i32);
        let mut b = ((0.0, 0.0), (2.0, 3.0), (0.0, 0.0), 100_i32);
        for _ in 0..100 {
            step_gravity(&mut a.0, &mut a.1, &mut a.2, &mut a.3, &planets);
            step_gravity(&mut b.0, &mut b.1, &mut b.2, &mut b.3, &planets);
        }
        assert_eq!(a, b);
    }

    #[test]
    fn circle_line_intersect_hit_and_miss() {
        // Horizontal segment through a circle at (10, 0), r = 5.
        let hit = circle_line_intersect((10.0, 0.0), 5.0, (0.0, 0.0), (20.0, 0.0));
        // The impact point is pulled back slightly along the segment (alpha - 0.05).
        assert!((hit.0 - 14.0).abs() < 1e-9, "got {hit:?}");
        assert!(hit.1.abs() < 1e-9);
        // Segment far from the circle: sentinel miss position.
        let miss = circle_line_intersect((10.0, 100.0), 5.0, (0.0, 0.0), (20.0, 0.0));
        assert_eq!(miss, (2000.0, 1500.0));
    }

    #[test]
    fn generate_planets_is_deterministic_for_seed() {
        let settings = GameSettingsData::default();
        let mut a = StdRng::seed_from_u64(42);
        let mut b = StdRng::seed_from_u64(42);
        assert_eq!(generate_planets(&settings, &mut a), generate_planets(&settings, &mut b));
    }

    #[test]
    fn generate_planets_respects_count_and_bounds() {
        let settings = GameSettingsData {
            max_planets: 4,
            ..Default::default()
        };
        let mut rng = StdRng::seed_from_u64(7);
        let planets = generate_planets(&settings, &mut rng);
        assert!((2..=4).contains(&planets.len()), "got {}", planets.len());
        for p in &planets {
            assert!(p.pos.0.abs() <= WORLD_HALF_W && p.pos.1.abs() <= WORLD_HALF_H);
            assert!(!p.is_blackhole);
        }
    }

    #[test]
    fn generate_planets_never_overlap() {
        let settings = GameSettingsData {
            max_planets: 4,
            ..Default::default()
        };
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let planets = generate_planets(&settings, &mut rng);
            for (i, a) in planets.iter().enumerate() {
                for b in planets.iter().skip(i + 1) {
                    let d = ((a.pos.0 - b.pos.0).powi(2) + (a.pos.1 - b.pos.1).powi(2)).sqrt();
                    let min = (a.radius + b.radius) * PLANET_OVERLAP_SCALE
                        + PLANET_OVERLAP_MASS_K * (a.mass + b.mass);
                    assert!(d >= min, "seed {seed}: planets overlap");
                }
            }
        }
    }

    #[test]
    fn generate_blackholes_when_requested() {
        let settings = GameSettingsData {
            max_blackholes: 3,
            ..Default::default()
        };
        let mut rng = StdRng::seed_from_u64(3);
        let planets = generate_planets(&settings, &mut rng);
        assert!(!planets.is_empty());
        assert!(planets.len() <= 3);
        assert!(planets.iter().all(|p| p.is_blackhole));
    }

    #[test]
    fn apply_bounce_reflects_velocity() {
        let mut body = BodySnapshot {
            pos: (BOUNCE_X_MAX + 5.0, 0.0),
            vel: (10.0, 0.0),
            last_pos: (BOUNCE_X_MAX - 1.0, 0.0),
            flight: 10,
            active: true,
        };
        apply_bounce(&mut body);
        assert_eq!(body.pos.0, BOUNCE_X_MAX);
        assert_eq!(body.vel.0, -10.0);
    }
}
