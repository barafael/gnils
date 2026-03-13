use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

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
                // Avoid division by zero (same as Python's ZeroDivisionError catch)
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
}

/// Apply gravity from all planets to particles.
pub fn particle_gravity(
    mut particles: Query<(&mut GravityBody, &ParticleMarker)>,
    planets: Query<&Planet>,
) {
    // Particles always update when this system runs (scheduling handles state gating)

    for (mut body, _) in particles.iter_mut() {
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
}

/// Sync GravityBody positions to Bevy Transform for rendering.
pub fn sync_transforms(
    mut query: Query<
        (&GravityBody, &mut Transform, &mut Visibility),
        Or<(With<MissileMarker>, With<ParticleMarker>)>,
    >,
) {
    for (body, mut transform, mut visibility) in query.iter_mut() {
        let bx = body.pos.0 as f32 - WINDOW_WIDTH / 2.0;
        let by = WINDOW_HEIGHT / 2.0 - body.pos.1 as f32;
        transform.translation.x = bx;
        transform.translation.y = by;

        // Check if visible on screen (within 800x600 pygame coords)
        let in_screen =
            body.pos.0 >= 0.0 && body.pos.0 <= 800.0 && body.pos.1 >= 0.0 && body.pos.1 <= 600.0;

        // For particles, hide if out of extended range
        // For missile, visibility is handled elsewhere
        if in_screen {
            *visibility = Visibility::Inherited;
        }
    }
}
