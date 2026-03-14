use bevy::prelude::*;
use rand::Rng;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;
use crate::systems::player::pygame_to_bevy;

/// Process queued particle spawn requests.
pub fn spawn_particles(
    mut commands: Commands,
    mut spawn_queue: ResMut<ParticleSpawnQueue>,
    assets: Res<GameAssets>,
    settings: Res<GameSettings>,
) {
    if !settings.particles_enabled {
        spawn_queue.requests.clear();
        return;
    }

    let mut rng = rand::thread_rng();

    let requests = std::mem::take(&mut spawn_queue.requests);
    if !requests.is_empty() {
        info!("Spawning particles: {} requests", requests.len());
    }
    for request in requests {
        let count = if settings.bounce {
            request.count / 2
        } else {
            request.count
        };

        for _ in 0..count {
            let angle = rng.gen_range(0..360) as f64;
            let speed = if request.size == 5 {
                rng.gen_range(PARTICLE_5_MIN_SPEED..=PARTICLE_5_MAX_SPEED)
            } else {
                rng.gen_range(PARTICLE_10_MIN_SPEED..=PARTICLE_10_MAX_SPEED)
            };

            let vx = 0.1 * speed * (angle.to_radians()).sin();
            let vy = -0.1 * speed * (angle.to_radians()).cos();

            let pos = (request.pos.x as f64, request.pos.y as f64);

            let texture = if request.size == 5 {
                assets.explosion_5.clone()
            } else {
                assets.explosion_10.clone()
            };

            let bevy_pos = pygame_to_bevy(pos.0, pos.1);

            commands.spawn((
                Sprite::from_image(texture),
                Transform::from_xyz(bevy_pos.x, bevy_pos.y, 5.0),
                GravityBody {
                    pos,
                    velocity: (vx, vy),
                    last_pos: pos,
                    flight: MAX_FLIGHT,
                },
                ParticleMarker { size: request.size },
            ));
        }
    }
}

/// Clean up particles that are expired or out of range.
pub fn cleanup_particles(
    mut commands: Commands,
    particles: Query<(Entity, &GravityBody), With<ParticleMarker>>,
) {
    for (entity, body) in particles.iter() {
        if body.flight < 0 || !is_in_extended_range(body.pos) {
            commands.entity(entity).despawn();
        }
    }
}
