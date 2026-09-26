use bevy::prelude::*;

use gnils_protocol::{BOUNCE_X_MAX, BOUNCE_X_MIN, BOUNCE_Y_MAX, BOUNCE_Y_MIN, PlanetData};

use crate::components::*;
use crate::resources::*;

// ── Gravity helpers ────────────────────────────────────────────────────────

/// Snapshot the planets for the shared pure-Rust gravity step. Built once per
/// system run and reused for every body it moves.
fn planet_data(planets: &Query<&Planet>) -> Vec<PlanetData> {
    planets
        .iter()
        .map(|p| PlanetData {
            mass: p.mass,
            radius: p.radius,
            pos: (p.pos.x as f64, p.pos.y as f64),
            is_blackhole: p.is_blackhole,
            texture_index: 0,
        })
        .collect()
}

fn apply_gravity(body: &mut GravityBody, planets: &[PlanetData]) {
    gnils_protocol::step_gravity(
        &mut body.pos,
        &mut body.velocity,
        &mut body.last_pos,
        &mut body.flight,
        planets,
    );
}

/// Reflect one axis off `limit`: put the crossing point back on the wall,
/// carry the other axis to where it was as the body crossed, and flip the
/// velocity.
fn reflect(pos: &mut f64, last: f64, other: &mut f64, last_other: f64, vel: &mut f64, limit: f64) {
    let d = *pos - last;
    if d.abs() > 1e-10 {
        *other = last_other + (*other - last_other) * (limit - last) / d;
    }
    *pos = limit;
    *vel = -*vel;
}

/// Apply bounce reflection to a GravityBody at the screen edges.
/// Used by both missiles and particles when BOUNCE mode is on.
pub fn bounce_gravity_body(b: &mut GravityBody) {
    let (pos, last, vel) = (&mut b.pos, b.last_pos, &mut b.velocity);
    if pos.0 > BOUNCE_X_MAX {
        reflect(&mut pos.0, last.0, &mut pos.1, last.1, &mut vel.0, BOUNCE_X_MAX);
    }
    if pos.0 < BOUNCE_X_MIN {
        reflect(&mut pos.0, last.0, &mut pos.1, last.1, &mut vel.0, BOUNCE_X_MIN);
    }
    if pos.1 > BOUNCE_Y_MAX {
        reflect(&mut pos.1, last.1, &mut pos.0, last.0, &mut vel.1, BOUNCE_Y_MAX);
    }
    if pos.1 < BOUNCE_Y_MIN {
        reflect(&mut pos.1, last.1, &mut pos.0, last.0, &mut vel.1, BOUNCE_Y_MIN);
    }
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
    let planets = planet_data(&planets);
    for (mut body, marker) in missile_q.iter_mut() {
        if marker.active {
            apply_gravity(&mut body, &planets);
        }
    }
}

/// Apply gravity from all planets to particles.
pub fn particle_gravity(
    mut particles: Query<&mut GravityBody, With<ParticleMarker>>,
    planets: Query<&Planet>,
) {
    let planets = planet_data(&planets);
    for mut body in particles.iter_mut() {
        apply_gravity(&mut body, &planets);
    }
}

/// Bounce particles off screen edges in BOUNCE mode.
pub fn particle_bounce(
    mut particles: Query<&mut GravityBody, With<ParticleMarker>>,
    settings: Res<GameSettings>,
) {
    if !settings.bounce {
        return;
    }
    for mut body in particles.iter_mut() {
        bounce_gravity_body(&mut body);
    }
}

/// Sync GravityBody positions to Bevy Transform for rendering.
pub fn sync_transforms(
    mut missiles: Query<(&GravityBody, &MissileMarker, &mut Transform), Without<ParticleMarker>>,
    mut particles: Query<(&GravityBody, &mut Transform), With<ParticleMarker>>,
) {
    let place = |body: &GravityBody, transform: &mut Transform| {
        transform.translation.x = body.pos.0 as f32;
        transform.translation.y = body.pos.1 as f32;
    };
    for (body, marker, mut transform) in missiles.iter_mut() {
        if marker.active {
            place(body, &mut transform);
        }
    }
    for (body, mut transform) in particles.iter_mut() {
        place(body, &mut transform);
    }
}
