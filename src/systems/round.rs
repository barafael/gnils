use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::events::HitType;
use crate::resources::*;

/// Handle queued missile impacts: scoring, marking players as shot, etc.
pub fn handle_missile_impact(
    mut impact_queue: ResMut<MissileImpactQueue>,
    mut players: Query<&mut Player>,
    missile_q: Query<&MissileMarker>,
    mut turn: ResMut<TurnState>,
    mut spawn_queue: ResMut<ParticleSpawnQueue>,
    settings: Res<GameSettings>,
) {
    let impacts: Vec<_> = impact_queue.impacts.drain(..).collect();
    for impact in impacts {
        match impact.hit_type {
            HitType::Planet => {
                info!("Missile hit planet at ({}, {})", impact.pos.x, impact.pos.y);
                if settings.particles_enabled {
                    spawn_queue.requests.push(ParticleSpawnRequest {
                        pos: impact.pos,
                        count: N_PARTICLES_10,
                        size: 10,
                    });
                }
                end_shot(&mut turn);
            }
            HitType::Blackhole => {
                info!("Missile absorbed by blackhole");
                end_shot(&mut turn);
            }
            HitType::Ship(hit_id) => {
                for mut player in players.iter_mut() {
                    if player.id == hit_id {
                        player.shot = true;
                    }
                }

                if settings.particles_enabled {
                    spawn_queue.requests.push(ParticleSpawnRequest {
                        pos: impact.pos,
                        count: N_PARTICLES_10,
                        size: 10,
                    });
                }

                let last = turn.last_player;
                let power_penalty = missile_q
                    .iter()
                    .next()
                    .map(|m| m.power_penalty)
                    .unwrap_or(0);

                if last == hit_id {
                    for mut player in players.iter_mut() {
                        if player.id == hit_id {
                            player.score -= SELF_HIT;
                        }
                    }
                } else {
                    let mut bonus = 0;
                    for player in players.iter() {
                        if player.id == last {
                            bonus = match player.attempts {
                                1 => QUICK_SCORE_1,
                                2 => QUICK_SCORE_2,
                                3 => QUICK_SCORE_3,
                                _ => 0,
                            };
                        }
                    }

                    let score = power_penalty + bonus + HIT_SCORE;
                    for mut player in players.iter_mut() {
                        if player.id == last {
                            player.score += score;
                        }
                    }
                }

                info!("Ship {} hit! Round over.", hit_id);
                turn.round_over = true;
                turn.firing = false;
                turn.show_round = 100.0;
            }
            HitType::Timeout => {
                end_shot(&mut turn);
            }
        }
    }
}

fn end_shot(turn: &mut TurnState) {
    let next = 3 - turn.last_player;
    info!("End shot: next player = {}", next);
    turn.current_player = next;
    turn.firing = false;
}

/// This function is no longer needed since round_over_input handles it directly.
/// Kept as a stub for the main.rs reference.
pub fn handle_advance_round() {}

/// Round setup: increment round counter and transition to aiming.
pub fn round_setup(
    mut turn: ResMut<TurnState>,
    mut next_state: ResMut<NextState<GamePhase>>,
    settings: Res<GameSettings>,
) {
    turn.round += 1;

    if settings.invisible {
        turn.show_planets = 100.0;
    } else {
        turn.show_planets = 0.0;
    }

    next_state.set(GamePhase::Aiming);
}
