use bevy::prelude::*;
use gnils_protocol::compute_shot_score;

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
    mut round_result: ResMut<RoundResult>,
) {
    let impacts = std::mem::take(&mut impact_queue.impacts);
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
                let last = turn.last_player;
                let killed_self = last == hit_id;
                let power_penalty = missile_q
                    .iter()
                    .next()
                    .map(|m| m.power_penalty)
                    .unwrap_or(0);

                let mut shooter_attempts = 0u32;
                for mut player in players.iter_mut() {
                    if player.id == hit_id {
                        player.shot = true;
                    }
                    if player.id == last {
                        shooter_attempts = player.attempts;
                    }
                }

                if settings.particles_enabled {
                    spawn_queue.requests.push(ParticleSpawnRequest {
                        pos: impact.pos,
                        count: N_PARTICLES_10,
                        size: 10,
                    });
                }
                let (total_delta, quick_bonus, pen) =
                    compute_shot_score(killed_self, power_penalty, shooter_attempts);

                for mut player in players.iter_mut() {
                    if player.id == (if killed_self { hit_id } else { last }) {
                        player.score += total_delta;
                    }
                }
                *round_result = RoundResult {
                    hit_player: hit_id,
                    shooter: last,
                    self_hit: killed_self,
                    hit_score: if killed_self { -SELF_HIT } else { HIT_SCORE },
                    quick_bonus,
                    power_penalty: pen,
                    total_score: total_delta,
                    message: if killed_self {
                        format!("Player {} hit themselves!", last)
                    } else {
                        format!("Player {} hits Player {}!", last, hit_id)
                    },
                };

                info!("Ship {} hit! Round over.", hit_id);
                turn.round_over = true;
                turn.firing = false;

                // Check if this was the final round — only game over gets the zoom text
                if settings.max_rounds > 0 && turn.round >= settings.max_rounds {
                    turn.game_over = true;
                    turn.show_round = 100.0;
                }
            }
        }
    }
}

fn end_shot(turn: &mut TurnState) {
    let next = turn.other_player();
    info!("End shot: next player = {}", next);
    turn.current_player = next;
    turn.firing = false;
}

/// Round setup: increment round counter, randomize player positions, and transition to aiming.
pub fn round_setup(
    mut turn: ResMut<TurnState>,
    mut next_state: ResMut<NextState<GamePhase>>,
    settings: Res<GameSettings>,
    mut players: Query<(&mut Player, &mut Transform)>,
) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    turn.round += 1;

    // Randomize player Y positions each round
    for (player, mut transform) in players.iter_mut() {
        let y = rng.gen_range(PLAYER_Y_MIN..=PLAYER_Y_MAX);
        let x = if player.id == 1 { PLAYER1_X } else { PLAYER2_X };
        transform.translation.x = x as f32;
        transform.translation.y = y as f32;
    }

    if settings.invisible {
        turn.show_planets = 100.0;
    } else {
        turn.show_planets = 0.0;
    }

    next_state.set(GamePhase::Aiming);
}
