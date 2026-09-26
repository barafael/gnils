use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

use gnils_protocol::circle_line_intersect;

/// The first body the point has fallen into, if any.
fn body_at<'a>(planets: &'a Query<&Planet>, pos: (f64, f64)) -> Option<&'a Planet> {
    planets.iter().find(|p| p.contains(pos))
}

/// Check missile collision with planets, ships, and boundaries.
#[allow(clippy::too_many_arguments)]
pub fn missile_collision(
    mut missile_q: Query<(&mut GravityBody, &mut MissileMarker)>,
    planets: Query<&Planet>,
    players: Query<(&Player, &Transform)>,
    blended: Res<BlendedShipImages>,
    images: Res<Assets<Image>>,
    mut impact_queue: ResMut<MissileImpactQueue>,
    settings: Res<GameSettings>,
    mut turn: ResMut<TurnState>,
) {
    for (mut body, mut marker) in missile_q.iter_mut() {
        if !marker.active {
            continue;
        }

        // A shot that runs out of fuel off-screen, or leaves the extended
        // playfield entirely, is simply over: the turn passes.
        let lost = if body.flight < 0 && !is_on_screen(body.pos) {
            Some("timed out (off-screen)")
        } else if !is_in_extended_range(body.pos) {
            Some("out of range")
        } else {
            None
        };
        if let Some(why) = lost {
            info!("Missile {why}");
            marker.active = false;
            turn.firing = false;
            turn.current_player = turn.other_player();
            continue;
        }

        // Planet or blackhole. `turn.firing` is left true — the impact
        // handler clears it once it has scored the hit.
        if let Some(planet) = body_at(&planets, body.pos) {
            let (pos, hit_type) = if planet.is_blackhole {
                ((planet.pos.x as f64, planet.pos.y as f64), HitType::Blackhole)
            } else {
                // Back the missile up to where it met the surface.
                let impact =
                    circle_line_intersect(
                        (planet.pos.x as f64, planet.pos.y as f64),
                        planet.radius,
                        body.last_pos,
                        body.pos,
                    );
                body.pos = impact;
                (impact, HitType::Planet)
            };
            impact_queue.impacts.push(MissileImpact {
                pos: Vec2::new(pos.0 as f32, pos.1 as f32),
                hit_type,
            });
            marker.active = false;
            marker.draw_last_segment = !planet.is_blackhole;
            return;
        }

        if let Some((hit_id, pos)) = ship_hit(&body, &players, &blended, &images) {
            body.pos = pos;
            impact_queue.impacts.push(MissileImpact {
                pos: Vec2::new(pos.0 as f32, pos.1 as f32),
                hit_type: HitType::Ship(hit_id),
            });
            marker.active = false;
            marker.draw_last_segment = true;
            return;
        }

        // Bounce mode — use the shared bounce helper.
        if settings.bounce {
            crate::systems::physics::bounce_gravity_body(&mut body);
        }
    }
}

/// Pixel-perfect ship collision along the step the missile just took.
///
/// The sub-step point is inverse-rotated into the ship's local (unrotated)
/// frame and the blended ship texture's alpha sampled there, matching the
/// original's `Player.hit()` pixel test. No grace period is needed: the
/// gun-tip pixels are transparent, so a launch cannot self-collide.
fn ship_hit(
    body: &GravityBody,
    players: &Query<(&Player, &Transform)>,
    blended: &BlendedShipImages,
    images: &Assets<Image>,
) -> Option<(u8, (f64, f64))> {
    let half_w = SHIP_FRAME_WIDTH as f64 / 2.0;
    let half_h = SHIP_FRAME_HEIGHT as f64 / 2.0;

    for (player, transform) in players.iter() {
        let cx = transform.translation.x as f64;
        let cy = transform.translation.y as f64;
        let (sin_r, cos_r) = player.rel_rot.sin_cos();
        let image = images.get(&blended.handles[(player.id - 1) as usize]);

        for i in 0..10 {
            let px = body.last_pos.0 + i as f64 * 0.1 * body.velocity.0;
            let py = body.last_pos.1 + i as f64 * 0.1 * body.velocity.1;

            // Quick AABB reject before the expensive pixel test.
            if (px - cx).abs() > half_w || (py - cy).abs() > half_h {
                continue;
            }

            // Ship-local space, then image pixels (origin top-left, Y-down).
            let (dx, dy) = (px - cx, py - cy);
            let pix_x = dx * cos_r + dy * sin_r + half_w;
            let pix_y = half_h - (-dx * sin_r + dy * cos_r);
            if !(0.0..SHIP_FRAME_WIDTH as f64).contains(&pix_x)
                || !(0.0..SHIP_FRAME_HEIGHT as f64).contains(&pix_y)
            {
                continue;
            }

            // Without the texture there is nothing to sample: fall back to
            // the bounding box we already passed.
            let opaque = match image.and_then(|i| i.data.as_ref()) {
                Some(data) => {
                    let stride = SHIP_FRAME_WIDTH as usize * 4;
                    data[pix_y as usize * stride + pix_x as usize * 4 + 3] > 0
                }
                None => true,
            };
            if opaque {
                return Some((player.id, (px, py)));
            }
        }
    }
    None
}

/// Check particle collision with planets.
pub fn particle_collision(
    mut commands: Commands,
    particles: Query<(Entity, &GravityBody, &ParticleMarker)>,
    planets: Query<&Planet>,
    mut spawn_queue: ResMut<ParticleSpawnQueue>,
) {
    for (entity, body, particle) in particles.iter() {
        if body.flight < 0 || !is_in_extended_range(body.pos) {
            commands.entity(entity).despawn();
            continue;
        }

        let Some(planet) = body_at(&planets, body.pos) else {
            continue;
        };
        // A large fragment shatters into small ones on a planet's surface;
        // a blackhole swallows everything whole.
        if !planet.is_blackhole && particle.size == 10 {
            spawn_queue.requests.push(ParticleSpawnRequest {
                pos: Vec2::new(body.pos.0 as f32, body.pos.1 as f32),
                count: N_PARTICLES_5,
                size: 5,
            });
        }
        commands.entity(entity).despawn();
    }
}
