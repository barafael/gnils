use bevy::prelude::*;

use crate::components::*;
use crate::resources::*;

// ── Gravity helpers ────────────────────────────────────────────────────────

/// Collect planet data from ECS and call the shared pure-Rust gravity step.
fn apply_gravity(body: &mut GravityBody, planets: &Query<&Planet>) {
    let planet_data: Vec<gnils_protocol::PlanetData> = planets
        .iter()
        .map(|p| gnils_protocol::PlanetData {
            mass: p.mass,
            radius: p.radius,
            pos: (p.pos.x as f64, p.pos.y as f64),
            is_blackhole: p.is_blackhole,
            texture_index: 0,
        })
        .collect();

    gnils_protocol::step_gravity(
        &mut body.pos,
        &mut body.velocity,
        &mut body.last_pos,
        &mut body.flight,
        &planet_data,
    );
}

// ── ECS systems ────────────────────────────────────────────────────────────

/// Apply gravity from all planets to the active missile.
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
pub fn sync_transforms(
    mut missiles: Query<(&GravityBody, &MissileMarker, &mut Transform), Without<ParticleMarker>>,
    mut particles: Query<(&GravityBody, &mut Transform), With<ParticleMarker>>,
) {
    for (body, marker, mut transform) in missiles.iter_mut() {
        if !marker.active {
            continue;
        }
        transform.translation.x = body.pos.0 as f32;
        transform.translation.y = body.pos.1 as f32;
    }

    for (body, mut transform) in particles.iter_mut() {
        transform.translation.x = body.pos.0 as f32;
        transform.translation.y = body.pos.1 as f32;
    }
}
