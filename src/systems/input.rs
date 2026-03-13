use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

pub fn aiming_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut turn: ResMut<TurnState>,
    mut players: Query<&mut Player>,
) {
    if turn.round_over || turn.firing {
        return;
    }

    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);

    let (power_step, angle_step) = if ctrl {
        (1.0, 0.25)
    } else if shift {
        (25.0, 5.0)
    } else if alt {
        (0.2, 0.05)
    } else {
        (10.0, 2.0)
    };

    let current = turn.current_player;

    for mut player in players.iter_mut() {
        if player.id != current {
            continue;
        }

        if keys.pressed(KeyCode::ArrowUp) {
            player.power = (player.power + power_step).min(MAX_POWER);
        }
        if keys.pressed(KeyCode::ArrowDown) {
            player.power = (player.power - power_step).max(0.0);
        }
        if keys.pressed(KeyCode::ArrowLeft) {
            player.angle -= angle_step;
            player.rel_rot -= angle_step;
            if player.angle < 0.0 {
                player.angle += 360.0;
            }
            if player.rel_rot < 0.0 {
                player.rel_rot += 360.0;
            }
        }
        if keys.pressed(KeyCode::ArrowRight) {
            player.angle += angle_step;
            player.rel_rot += angle_step;
            if player.angle >= 360.0 {
                player.angle -= 360.0;
            }
            if player.rel_rot >= 360.0 {
                player.rel_rot -= 360.0;
            }
        }

        if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
            // Signal that we want to fire - the fire_missile system will handle the rest
            turn.firing = true;
        }
    }
}

pub fn round_over_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut turn: ResMut<TurnState>,
    mut players: Query<&mut Player>,
    mut missile_q: Query<(&mut MissileMarker, &mut Visibility), Without<Player>>,
    settings: Res<GameSettings>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut next_state: ResMut<NextState<GamePhase>>,
) {
    if !turn.round_over {
        return;
    }

    if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
        // Handle advance round directly here instead of via event

        // Check if game over (max rounds reached)
        if settings.max_rounds > 0 && turn.round >= settings.max_rounds {
            turn.game_over = true;
            next_state.set(GamePhase::GameOver);
            return;
        }

        // Clear trail
        if let Some(image) = images.get_mut(&trail_canvas.image_handle) {
            crate::trail::clear_trail(image);
        }

        // Reset player states
        let mut p1_score = 0;
        let mut p2_score = 0;
        for player in players.iter() {
            if player.id == 1 {
                p1_score = player.score;
            } else {
                p2_score = player.score;
            }
        }

        // Lower score player goes first
        if p1_score < p2_score {
            turn.current_player = 1;
        } else if p2_score < p1_score {
            turn.current_player = 2;
        }
        // If equal, keep current order

        turn.round_over = false;
        turn.firing = false;
        turn.show_round = 100.0;

        // Reset players for new round
        for mut player in players.iter_mut() {
            player.power = 100.0;
            player.shot = false;
            player.attempts = 0;
            player.explosion_frame = 0;
            player.rel_rot = 0.01;
            if player.id == 1 {
                player.angle = 90.0;
            } else {
                player.angle = 270.0;
            }
        }

        // Reset missile
        for (mut marker, mut vis) in missile_q.iter_mut() {
            marker.active = false;
            *vis = Visibility::Hidden;
        }

        // Trigger round setup which will spawn new planets
        next_state.set(GamePhase::RoundSetup);
    }
}
