use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

/// Update player ship transforms based on position and rotation.
/// Selects the correct atlas frame and applies full rotation via Transform.
/// Original Python: selects frame from rel_rot, then does rotozoom(-rel_rot, 1.0).
pub fn update_player_sprites(mut players: Query<(&Player, &mut Transform, &mut Sprite)>) {
    for (player, mut transform, mut sprite) in players.iter_mut() {
        // Python formula: img1 = round((rel_rot + 22.5) / 45 - 0.49) % 8
        // This selects one of 8 frames (each covering 45 degrees)
        let frame = ((player.rel_rot + 22.5) / 45.0 - 0.49).round() as i32;
        let frame = ((frame % 8) + 8) as usize % 8;

        if let Some(ref mut atlas) = sprite.texture_atlas {
            atlas.index = frame;
        }

        // Full rotation matching Python's rotozoom(-rel_rot, 1.0)
        // Bevy 2D: positive Z rotation = counter-clockwise, so negate for clockwise
        transform.rotation = Quat::from_rotation_z(-(player.rel_rot as f32).to_radians());
    }
}

/// Draw the aiming line using gizmos.
pub fn draw_aim_line(mut gizmos: Gizmos, players: Query<(&Player, &Transform)>, turn: Res<TurnState>) {
    if turn.firing || turn.round_over {
        return;
    }

    for (player, transform) in players.iter() {
        if player.id != turn.current_player {
            continue;
        }

        // Get launch point in pygame coords
        let (lx, ly) = get_launch_point_from_transform(player, transform);

        // End point of aim line
        let end_x = lx + player.power * player.angle.to_radians().sin();
        let end_y = ly - player.power * player.angle.to_radians().cos();

        // Convert both to bevy coords
        let start_bevy = pygame_to_bevy(lx, ly);
        let end_bevy = pygame_to_bevy(end_x, end_y);

        gizmos.line_2d(start_bevy, end_bevy, player.color());
    }
}

/// Update UI text for scores and angle/power.
pub fn update_ui_text(
    players: Query<&Player>,
    turn: Res<TurnState>,
    mut score_p1: Query<
        &mut Text,
        (
            With<UiScoreP1>,
            Without<UiScoreP2>,
            Without<UiAnglePower>,
            Without<UiRoundInfo>,
        ),
    >,
    mut score_p2: Query<
        &mut Text,
        (
            With<UiScoreP2>,
            Without<UiScoreP1>,
            Without<UiAnglePower>,
            Without<UiRoundInfo>,
        ),
    >,
    mut angle_power: Query<
        &mut Text,
        (
            With<UiAnglePower>,
            Without<UiScoreP1>,
            Without<UiScoreP2>,
            Without<UiRoundInfo>,
        ),
    >,
    mut round_info: Query<
        &mut Text,
        (
            With<UiRoundInfo>,
            Without<UiScoreP1>,
            Without<UiScoreP2>,
            Without<UiAnglePower>,
        ),
    >,
    settings: Res<GameSettings>,
) {
    for player in players.iter() {
        if player.id == 1 {
            if let Ok(mut text) = score_p1.single_mut() {
                **text = format!("Player 1  --  {}", player.score);
            }
        } else {
            if let Ok(mut text) = score_p2.single_mut() {
                **text = format!("{}  --  Player 2", player.score);
            }
        }

        if player.id == turn.current_player && !turn.firing && !turn.round_over {
            if let Ok(mut text) = angle_power.single_mut() {
                **text = format!("Angle: {:.2}  Power: {:.1}", player.angle, player.power);
            }
        }
    }

    if let Ok(mut text) = round_info.single_mut() {
        if settings.max_rounds > 0 {
            **text = format!("Round {} of {}", turn.round, settings.max_rounds);
        } else {
            **text = format!("Round {}", turn.round);
        }
    }
}

/// Get launch point using player data and transform.
pub fn get_launch_point_from_transform(player: &Player, transform: &Transform) -> (f64, f64) {
    let cx = (transform.translation.x + WINDOW_WIDTH / 2.0) as f64;
    let cy = (WINDOW_HEIGHT / 2.0 - transform.translation.y) as f64;
    let angle_rad = player.angle.to_radians();
    (
        cx + player.gun_offset * angle_rad.sin(),
        cy - player.gun_offset * angle_rad.cos(),
    )
}

/// Convert pygame coordinates (top-left origin, Y-down) to bevy coordinates (center origin, Y-up).
pub fn pygame_to_bevy(x: f64, y: f64) -> Vec2 {
    Vec2::new(
        x as f32 - WINDOW_WIDTH / 2.0,
        WINDOW_HEIGHT / 2.0 - y as f32,
    )
}
