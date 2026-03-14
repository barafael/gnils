use bevy::prelude::*;
use gnils_protocol::{generate_planets, PlanetData};
use rand::thread_rng;

use crate::components::Planet;
use crate::resources::*;

pub fn spawn_planets(
    mut commands: Commands,
    assets: Res<GameAssets>,
    settings: Res<GameSettings>,
    existing_planets: Query<Entity, With<Planet>>,
) {
    for entity in existing_planets.iter() {
        commands.entity(entity).despawn();
    }

    let mut rng = thread_rng();
    let planets = generate_planets(&settings.to_protocol(), &mut rng);
    spawn_planet_entities(&mut commands, &assets, &planets);
}

/// Spawn Bevy entities for a slice of `PlanetData` (used by both local play and network setup).
pub fn spawn_planet_entities(commands: &mut Commands, assets: &GameAssets, planets: &[PlanetData]) {
    for planet in planets {
        let px = planet.pos.0 as f32;
        let py = planet.pos.1 as f32;

        if planet.is_blackhole {
            commands.spawn((
                Sprite {
                    color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                    custom_size: Some(Vec2::new(2.0, 2.0)),
                    ..default()
                },
                Transform::from_xyz(px, py, 2.0),
                Planet {
                    mass: planet.mass,
                    radius: planet.radius,
                    pos: Vec2::new(px, py),
                    is_blackhole: true,
                },
            ));
        } else {
            let ti = planet.texture_index as usize;
            let sprite_size = (2.0 * planet.radius / 0.96) as f32;
            commands.spawn((
                Sprite {
                    image: assets.planets[ti].clone(),
                    custom_size: Some(Vec2::new(sprite_size, sprite_size)),
                    ..default()
                },
                Transform::from_xyz(px, py, 2.0),
                Planet {
                    mass: planet.mass,
                    radius: planet.radius,
                    pos: Vec2::new(px, py),
                    is_blackhole: false,
                },
            ));
        }
    }
}
