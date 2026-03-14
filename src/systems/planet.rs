use bevy::prelude::*;
use rand::Rng;

use crate::components::Planet;
use crate::constants::*;
use crate::resources::*;

pub fn spawn_planets(
    mut commands: Commands,
    assets: Res<GameAssets>,
    settings: Res<GameSettings>,
    existing_planets: Query<Entity, With<Planet>>,
) {
    // Despawn existing planets
    for entity in existing_planets.iter() {
        commands.entity(entity).despawn();
    }

    let mut rng = rand::thread_rng();

    // Track all placed bodies (x, y, radius, mass) for overlap checks
    let mut placed_bodies: Vec<(f64, f64, f64, f64)> = Vec::new();

    // Spawn blackholes first (if enabled)
    if settings.max_blackholes > 0 {
        let n_blackholes = rng.gen_range(1..=settings.max_blackholes);
        for _ in 0..n_blackholes {
            let mut placed = false;
            let mut attempts = 0;

            while !placed && attempts < 1000 {
                attempts += 1;

                let mass = rng.gen_range(600.0..=700.0_f64);
                let radius = 1.0_f64;

                // Blackholes use 3x distance from edges
                let x = rng.gen_range(
                    (-400.0 + 3.0 * PLANET_SHIP_DISTANCE + radius)
                        ..=(400.0 - 3.0 * PLANET_SHIP_DISTANCE - radius),
                );
                let y = rng.gen_range(
                    (-300.0 + 3.0 * PLANET_EDGE_DISTANCE + radius)
                        ..=(300.0 - 3.0 * PLANET_EDGE_DISTANCE - radius),
                );

                let mut ok = true;
                for &(px, py, pr, pm) in &placed_bodies {
                    let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                    if d < (radius + pr) * 1.5 + 0.1 * (mass + pm) {
                        ok = false;
                        break;
                    }
                }

                if ok {
                    placed_bodies.push((x, y, radius, mass));

                    // Blackholes are invisible (2x2 transparent sprite)
                    commands.spawn((
                        Sprite {
                            color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                            custom_size: Some(Vec2::new(2.0, 2.0)),
                            ..default()
                        },
                        Transform::from_xyz(x as f32, y as f32, 2.0),
                        Planet {
                            mass,
                            radius,
                            pos: Vec2::new(x as f32, y as f32),
                            is_blackhole: true,
                        },
                    ));
                    placed = true;
                }
            }
        }
    }

    // Spawn normal planets (only if no blackholes, matching Python behavior)
    if settings.max_blackholes == 0 {
        let n_planets = rng.gen_range(2..=settings.max_planets);
        let mut used_ns: Vec<u8> = Vec::new();

        for _ in 0..n_planets {
            let mut placed = false;
            let mut attempts = 0;

            while !placed && attempts < 1000 {
                attempts += 1;

                let mass = rng.gen_range(8.0..=512.0_f64);
                let radius = mass.powf(1.0 / 3.0) * 12.5;

                let x = rng.gen_range(
                    (-400.0 + PLANET_SHIP_DISTANCE + radius)..=(400.0 - PLANET_SHIP_DISTANCE - radius),
                );
                let y = rng.gen_range(
                    (-300.0 + PLANET_EDGE_DISTANCE + radius)..=(300.0 - PLANET_EDGE_DISTANCE - radius),
                );

                let mut ok = true;
                for &(px, py, pr, pm) in &placed_bodies {
                    let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                    if d < (radius + pr) * 1.5 + 0.1 * (mass + pm) {
                        ok = false;
                        break;
                    }
                }

                if ok {
                    let mut n = rng.gen_range(1..=8u8);
                    let mut n_attempts = 0;
                    while used_ns.contains(&n) && n_attempts < 20 {
                        n = rng.gen_range(1..=8u8);
                        n_attempts += 1;
                    }
                    used_ns.push(n);

                    placed_bodies.push((x, y, radius, mass));

                    let texture_index = (n - 1) as usize;
                    let sprite_size = (2.0 * radius / 0.96) as f32;

                    commands.spawn((
                        Sprite {
                            image: assets.planets[texture_index].clone(),
                            custom_size: Some(Vec2::new(sprite_size, sprite_size)),
                            ..default()
                        },
                        Transform::from_xyz(x as f32, y as f32, 2.0),
                        Planet {
                            mass,
                            radius,
                            pos: Vec2::new(x as f32, y as f32),
                            is_blackhole: false,
                        },
                    ));
                    placed = true;
                }
            }
        }
    }
}
