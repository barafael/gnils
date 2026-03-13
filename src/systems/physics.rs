use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;
use crate::systems::player::pygame_to_bevy;

/// Shared gravity calculation for any GravityBody.
fn apply_gravity(body: &mut GravityBody, planets: &Query<&Planet>) {
    body.last_pos = body.pos;
    body.flight -= 1;
    for planet in planets.iter() {
        let px = planet.pos.x as f64;
        let py = planet.pos.y as f64;
        let mass = planet.mass;
        let dx = body.pos.0 - px;
        let dy = body.pos.1 - py;
        let d = dx * dx + dy * dy;
        if d < 1e-10 {
            body.velocity.0 -= 10000.0;
            body.velocity.1 -= 10000.0;
            continue;
        }
        let d_sqrt = d.sqrt();
        let ax = (GRAVITY * mass * dx) / (d * d_sqrt);
        let ay = (GRAVITY * mass * dy) / (d * d_sqrt);
        body.velocity.0 -= ax;
        body.velocity.1 -= ay;
    }
    body.pos.0 += body.velocity.0;
    body.pos.1 += body.velocity.1;
}

/// Apply gravity from all planets to the missile.
pub fn missile_gravity(
    mut missile_q: Query<(&mut GravityBody, &MissileMarker)>,
    planets: Query<&Planet>,
    turn: Res<TurnState>,
) {
    if !turn.firing {
        return;
    }

    for (mut body, marker) in missile_q.iter_mut() {
        if !marker.active {
            continue;
        }
        apply_gravity(&mut body, &planets);
    }
}

/// Apply gravity from all planets to particles.
pub fn particle_gravity(
    mut particles: Query<(&mut GravityBody, &ParticleMarker)>,
    planets: Query<&Planet>,
) {
    for (mut body, _) in particles.iter_mut() {
        apply_gravity(&mut body, &planets);
    }
}

/// Sync GravityBody positions to Bevy Transform for rendering.
/// Only updates position — visibility is managed by dedicated systems.
pub fn sync_transforms(
    mut missiles: Query<(&GravityBody, &MissileMarker, &mut Transform), Without<ParticleMarker>>,
    mut particles: Query<(&GravityBody, &mut Transform), With<ParticleMarker>>,
) {
    for (body, marker, mut transform) in missiles.iter_mut() {
        if !marker.active {
            continue;
        }
        let bevy_pos = pygame_to_bevy(body.pos.0, body.pos.1);
        transform.translation.x = bevy_pos.x;
        transform.translation.y = bevy_pos.y;
    }

    for (body, mut transform) in particles.iter_mut() {
        let bevy_pos = pygame_to_bevy(body.pos.0, body.pos.1);
        transform.translation.x = bevy_pos.x;
        transform.translation.y = bevy_pos.y;
    }
}
