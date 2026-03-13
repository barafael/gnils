use bevy::prelude::*;
use rand::Rng;

use crate::components::Planet;
use crate::constants::*;
use crate::resources::*;
use crate::systems::player::pygame_to_bevy;

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
    let n_planets = rng.gen_range(2..=settings.max_planets);

    let mut planets: Vec<(f64, f64, f64, f64, u8)> = Vec::new(); // (x, y, radius, mass, n)
    let mut used_ns: Vec<u8> = Vec::new();

    for _ in 0..n_planets {
        let mut placed = false;
        let mut attempts = 0;

        while !placed && attempts < 1000 {
            attempts += 1;

            let mass = rng.gen_range(8.0..=512.0) as f64;
            let radius = mass.powf(1.0 / 3.0) * 12.5;

            let x = rng.gen_range(
                (PLANET_SHIP_DISTANCE + radius)..=(800.0 - PLANET_SHIP_DISTANCE - radius),
            );
            let y = rng.gen_range(
                (PLANET_EDGE_DISTANCE + radius)..=(600.0 - PLANET_EDGE_DISTANCE - radius),
            );

            // Check no overlap with existing planets
            let mut ok = true;
            for &(px, py, pr, pm, _) in &planets {
                let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                if d < (radius + pr) * 1.5 + 0.1 * (mass + pm) {
                    ok = false;
                    break;
                }
            }

            if ok {
                // Pick unique planet texture number
                let mut n = rng.gen_range(1..=8u8);
                let mut n_attempts = 0;
                while used_ns.contains(&n) && n_attempts < 20 {
                    n = rng.gen_range(1..=8u8);
                    n_attempts += 1;
                }
                used_ns.push(n);

                planets.push((x, y, radius, mass, n));
                placed = true;
            }
        }
    }

    // Spawn planet entities
    for &(x, y, radius, mass, n) in &planets {
        let texture_index = (n - 1) as usize;
        let sprite_size = (2.0 * radius / 0.96) as f32;

        let bevy_pos = pygame_to_bevy(x, y);

        commands.spawn((
            Sprite {
                image: assets.planets[texture_index].clone(),
                custom_size: Some(Vec2::new(sprite_size, sprite_size)),
                ..default()
            },
            Transform::from_xyz(bevy_pos.x, bevy_pos.y, 2.0),
            Planet {
                mass,
                radius,
                pos: Vec2::new(x as f32, y as f32),
                is_blackhole: false,
            },
        ));
    }
}
