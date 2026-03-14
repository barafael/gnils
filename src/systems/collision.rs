use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::events::HitType;
use crate::resources::*;

use gnils_protocol::circle_line_intersect;

/// Check missile collision with planets, ships, and boundaries.
pub fn missile_collision(
    mut missile_q: Query<(&mut GravityBody, &mut MissileMarker)>,
    planets: Query<&Planet>,
    players: Query<(&Player, &Transform)>,
    mut impact_queue: ResMut<MissileImpactQueue>,
    settings: Res<GameSettings>,
    mut turn: ResMut<TurnState>,
) {
    for (mut body, mut marker) in missile_q.iter_mut() {
        if !marker.active {
            continue;
        }

        // Check timeout
        if body.flight < 0 {
            if !is_on_screen(body.pos) {
                info!("Missile timed out (off-screen)");
                marker.active = false;
                turn.firing = false;
                turn.current_player = turn.other_player();
                continue;
            }
        }

        // Check out of extended range
        if !is_in_extended_range(body.pos) {
            info!("Missile out of range");
            marker.active = false;
            turn.firing = false;
            turn.current_player = turn.other_player();
            continue;
        }

        // Check planet collision
        for planet in planets.iter() {
            let px = planet.pos.x as f64;
            let py = planet.pos.y as f64;
            let r = planet.radius;
            let d_sq = (body.pos.0 - px).powi(2) + (body.pos.1 - py).powi(2);

            if planet.is_blackhole {
                if d_sq <= planet.mass * planet.mass {
                    let impact_pos = Vec2::new(px as f32, py as f32);
                    impact_queue.impacts.push(MissileImpact {
                        pos: impact_pos,
                        hit_type: HitType::Blackhole,
                    });
                    marker.active = false;
                    // turn.firing left true — handle_missile_impact will clear it
                    return;
                }
            } else if d_sq <= r * r {
                let impact = circle_line_intersect((px, py), r, body.last_pos, body.pos);
                body.pos = impact;
                let impact_pos = Vec2::new(impact.0 as f32, impact.1 as f32);
                impact_queue.impacts.push(MissileImpact {
                    pos: impact_pos,
                    hit_type: HitType::Planet,
                });
                marker.active = false;
                // turn.firing left true — handle_missile_impact will clear it
                return;
            }
        }

        // Check ship collision (sub-step check like original)
        for (player, transform) in players.iter() {
            // Skip the launching player for first few ticks to avoid self-hit on launch.
            // The original uses pixel-perfect detection which naturally avoids this since
            // the gun tip area is transparent. We use bounding box, so we need a grace period.
            if player.id == turn.last_player && body.flight > settings.max_flight - 5 {
                continue;
            }

            let cx = transform.translation.x as f64;
            let cy = transform.translation.y as f64;
            let half_w = SHIP_FRAME_WIDTH as f64 / 2.0;
            let half_h = SHIP_FRAME_HEIGHT as f64 / 2.0;

            for i in 0..10 {
                let px = body.last_pos.0 + i as f64 * 0.1 * body.velocity.0;
                let py = body.last_pos.1 + i as f64 * 0.1 * body.velocity.1;

                if px >= cx - half_w && px <= cx + half_w && py >= cy - half_h && py <= cy + half_h
                {
                    let impact_pos = Vec2::new(px as f32, py as f32);
                    body.pos = (px, py);
                    impact_queue.impacts.push(MissileImpact {
                        pos: impact_pos,
                        hit_type: HitType::Ship(player.id),
                    });
                    marker.active = false;
                    // turn.firing left true — handle_missile_impact will clear it
                    return;
                }
            }
        }

        // Bounce mode
        if settings.bounce {
            if body.pos.0 > 400.0 {
                let d = body.pos.0 - body.last_pos.0;
                if d.abs() > 1e-10 {
                    body.pos.1 = body.last_pos.1
                        + (body.pos.1 - body.last_pos.1) * (400.0 - body.last_pos.0) / d;
                }
                body.pos.0 = 400.0;
                body.velocity.0 = -body.velocity.0;
            }
            if body.pos.0 < -400.0 {
                let d = body.last_pos.0 - body.pos.0;
                if d.abs() > 1e-10 {
                    body.pos.1 = body.last_pos.1
                        + (body.pos.1 - body.last_pos.1) * (body.last_pos.0 + 400.0) / d;
                }
                body.pos.0 = -400.0;
                body.velocity.0 = -body.velocity.0;
            }
            if body.pos.1 > 300.0 {
                let d = body.pos.1 - body.last_pos.1;
                if d.abs() > 1e-10 {
                    body.pos.0 = body.last_pos.0
                        + (body.pos.0 - body.last_pos.0) * (300.0 - body.last_pos.1) / d;
                }
                body.pos.1 = 300.0;
                body.velocity.1 = -body.velocity.1;
            }
            if body.pos.1 < -300.0 {
                let d = body.last_pos.1 - body.pos.1;
                if d.abs() > 1e-10 {
                    body.pos.0 = body.last_pos.0
                        + (body.pos.0 - body.last_pos.0) * (body.last_pos.1 + 300.0) / d;
                }
                body.pos.1 = -300.0;
                body.velocity.1 = -body.velocity.1;
            }
        }
    }
}

/// Check particle collision with planets.
pub fn particle_collision(
    mut commands: Commands,
    mut particles: Query<(Entity, &mut GravityBody, &mut ParticleMarker)>,
    planets: Query<&Planet>,
    mut spawn_queue: ResMut<ParticleSpawnQueue>,
) {
    for (entity, body, particle) in particles.iter_mut() {
        if body.flight < 0 {
            commands.entity(entity).despawn();
            continue;
        }

        if !is_in_extended_range(body.pos) {
            commands.entity(entity).despawn();
            continue;
        }

        let mut hit = false;
        let mut hit_blackhole = false;
        for planet in planets.iter() {
            let px = planet.pos.x as f64;
            let py = planet.pos.y as f64;
            let d_sq = (body.pos.0 - px).powi(2) + (body.pos.1 - py).powi(2);

            if planet.is_blackhole {
                if d_sq <= (planet.mass * planet.mass) {
                    hit_blackhole = true;
                    break;
                }
            } else if d_sq <= planet.radius * planet.radius {
                hit = true;
                break;
            }
        }

        if hit_blackhole {
            commands.entity(entity).despawn();
        } else if hit {
            if particle.size == 10 {
                let impact_pos = Vec2::new(body.pos.0 as f32, body.pos.1 as f32);
                spawn_queue.requests.push(ParticleSpawnRequest {
                    pos: impact_pos,
                    count: N_PARTICLES_5,
                    size: 5,
                });
            }
            commands.entity(entity).despawn();
        }
    }
}

