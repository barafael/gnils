use bevy::prelude::*;
use gnils_protocol::{PlanetData, generate_planets};

use crate::components::Planet;
use crate::resources::*;
use crate::systems::round::layout_rng;

pub fn spawn_planets(
    mut commands: Commands,
    assets: Res<GameAssets>,
    settings: Res<GameSettings>,
    existing_planets: Query<Entity, With<Planet>>,
    turn: Res<TurnState>,
    net_seed: Option<Res<NetSeed>>,
    net_mode: Res<NetworkMode>,
) {
    for entity in existing_planets.iter() {
        commands.entity(entity).despawn();
    }

    // In network mode the layout comes from the shared seed so both peers
    // generate identical planets. Runs before `round_setup` increments the
    // round, so this uses the pre-increment round value (deterministic on both
    // peers either way).
    let mut rng = layout_rng(&net_mode, net_seed.as_deref(), turn.round);
    let planets = generate_planets(&settings.shared, &mut rng);
    spawn_planet_entities(&mut commands, &assets, &planets);
}

/// Spawn Bevy entities for a slice of `PlanetData`.
pub fn spawn_planet_entities(commands: &mut Commands, assets: &GameAssets, planets: &[PlanetData]) {
    for planet in planets {
        let pos = Vec2::new(planet.pos.0 as f32, planet.pos.1 as f32);
        // A blackhole has no texture — only a two-pixel invisible stand-in so
        // it still occupies an entity with a sprite.
        let sprite = if planet.is_blackhole {
            Sprite {
                color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                custom_size: Some(Vec2::splat(2.0)),
                ..default()
            }
        } else {
            Sprite {
                image: assets.planets[planet.texture_index as usize].clone(),
                custom_size: Some(Vec2::splat((2.0 * planet.radius / 0.96) as f32)),
                ..default()
            }
        };

        commands.spawn((
            sprite,
            Transform::from_xyz(pos.x, pos.y, 2.0),
            Planet {
                mass: planet.mass,
                radius: planet.radius,
                pos,
                is_blackhole: planet.is_blackhole,
            },
        ));
    }
}
