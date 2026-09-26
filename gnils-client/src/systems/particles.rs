use bevy::prelude::*;
use rand::RngExt;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

/// Process queued particle spawn requests.
pub fn spawn_particles(
    mut commands: Commands,
    mut spawn_queue: ResMut<ParticleSpawnQueue>,
    assets: Res<GameAssets>,
    settings: Res<GameSettings>,
) {
    let requests = std::mem::take(&mut spawn_queue.requests);
    if !settings.particles_enabled || requests.is_empty() {
        return;
    }
    info!("Spawning particles: {} requests", requests.len());

    let mut rng = rand::rng();
    for request in requests {
        let small = request.size == 5;
        let (texture, speeds) = if small {
            (
                assets.explosion_5.clone(),
                PARTICLE_5_MIN_SPEED..=PARTICLE_5_MAX_SPEED,
            )
        } else {
            (
                assets.explosion_10.clone(),
                PARTICLE_10_MIN_SPEED..=PARTICLE_10_MAX_SPEED,
            )
        };
        // Bounce mode keeps the debris around far longer, so halve it.
        let count = if settings.bounce {
            request.count / 2
        } else {
            request.count
        };
        let pos = (request.pos.x as f64, request.pos.y as f64);

        for _ in 0..count {
            let angle = rng.random_range(0..360) as f64;
            let speed = rng.random_range(speeds.clone());

            commands.spawn((
                Sprite::from_image(texture.clone()),
                Transform::from_xyz(request.pos.x, request.pos.y, 5.0),
                GravityBody {
                    pos,
                    velocity: (0.1 * speed * angle.sin(), 0.1 * speed * angle.cos()),
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
