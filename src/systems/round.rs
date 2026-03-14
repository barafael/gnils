use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::events::HitType;
use crate::resources::*;
use crate::systems::player::pygame_to_bevy;

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
                let killed_self = last == hit_id;
                let power_penalty = missile_q
                    .iter()
                    .next()
                    .map(|m| m.power_penalty)
                    .unwrap_or(0);

                if killed_self {
                    for mut player in players.iter_mut() {
                        if player.id == hit_id {
                            player.score -= SELF_HIT;
                        }
                    }
                    *round_result = RoundResult {
                        hit_player: hit_id,
                        shooter: last,
                        self_hit: true,
                        hit_score: -(SELF_HIT),
                        quick_bonus: 0,
                        power_penalty: 0,
                        total_score: -(SELF_HIT),
                        message: format!("Player {} hit themselves!", last),
                    };
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
                    *round_result = RoundResult {
                        hit_player: hit_id,
                        shooter: last,
                        self_hit: false,
                        hit_score: HIT_SCORE,
                        quick_bonus: bonus,
                        power_penalty,
                        total_score: score,
                        message: format!("Player {} hits Player {}!", last, hit_id),
                    };
                }

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

    // Randomize player Y positions each round (matching Python's player.init())
    for (player, mut transform) in players.iter_mut() {
        let y_pygame = rng.gen_range(PLAYER_Y_MIN..=PLAYER_Y_MAX);
        let x_pygame = if player.id == 1 { 40.0 } else { 760.0 };
        let bevy_pos = pygame_to_bevy(x_pygame, y_pygame);
        transform.translation.x = bevy_pos.x;
        transform.translation.y = bevy_pos.y;
    }

    if settings.invisible {
        turn.show_planets = 100.0;
    } else {
        turn.show_planets = 0.0;
    }

    next_state.set(GamePhase::Aiming);
}
