use bevy::prelude::*;
use gnils_protocol::compute_launch_point;

use crate::components::*;
use crate::resources::*;

/// Update player ship sprites via pixel-level frame blending (matching the
/// Python `change_angle` pipeline: blend two adjacent frames, then rotate).
pub fn update_player_sprites(
    mut players: Query<(&Player, &mut Transform, &mut Sprite)>,
    assets: Res<GameAssets>,
    mut images: ResMut<Assets<Image>>,
    blended: Res<BlendedShipImages>,
) {
    for (player, mut transform, mut sprite) in players.iter_mut() {
        if player.shot {
            continue;
        }

        let blended_handle = &blended.handles[(player.id - 1) as usize];

        // Restore ship sprite if it was replaced by explosion
        if sprite.image != *blended_handle {
            sprite.image = blended_handle.clone();
            sprite.texture_atlas = None;
            sprite.custom_size = None;
        }

        // Compute blend frames: img1 = primary frame, img2 = secondary, f = blend factor
        // rel_rot is in radians; compute_blend_frames expects degrees.
        let (img1, img2, blend_f) =
            crate::ship_blend::compute_blend_frames(player.rel_rot.to_degrees());

        let strip_handle = if player.id == 1 {
            &assets.red_ship
        } else {
            &assets.blue_ship
        };

        // Extract the two frames (releases borrow on images)
        let frames = images.get(strip_handle).map(|strip| {
            (
                crate::ship_blend::extract_frame(strip, img1),
                crate::ship_blend::extract_frame(strip, img2),
            )
        });

        if let Some((frame1, frame2)) = frames {
            let blended_img = crate::ship_blend::blend_frames(&frame1, &frame2, blend_f);
            if let Some(target) = images.get_mut(blended_handle) {
                *target = blended_img;
            }
        }

        sprite.color = Color::WHITE;
        // rel_rot is radians CCW from the ship's natural facing direction.
        transform.rotation = Quat::from_rotation_z(player.rel_rot as f32);
    }
}

/// Draw the aiming line using gizmos.
pub fn draw_aim_line(
    mut gizmos: Gizmos,
    players: Query<(&Player, &Transform)>,
    turn: Res<TurnState>,
    menu: Res<crate::resources::MenuOpen>,
) {
    if turn.firing || turn.round_over || menu.open {
        return;
    }

    for (player, transform) in players.iter() {
        if player.id != turn.current_player {
            continue;
        }

        let (lx, ly) = get_launch_point_from_transform(player, transform);
        let end_x = lx + player.power * player.angle.cos();
        let end_y = ly + player.power * player.angle.sin();

        gizmos.line_2d(
            Vec2::new(lx as f32, ly as f32),
            Vec2::new(end_x as f32, end_y as f32),
            player.color(),
        );
    }
}

/// Update ship explosion animation for hit players.
/// Uses half-frame increments since this runs at ~60fps but Python runs at 30fps.
pub fn update_ship_explosion(
    mut players: Query<(&mut Player, &mut Sprite, &mut Transform)>,
    assets: Res<GameAssets>,
) {
    for (mut player, mut sprite, mut transform) in players.iter_mut() {
        if !player.shot {
            continue;
        }

        // Increment at half speed to match 30fps original
        player.explosion_frame += 1;
        let e = player.explosion_frame as f64 * 0.5;
        let s = e * (6.0 - e) * 100.0 / 9.0;

        if s > 0.0 {
            if player.explosion_frame == 1 {
                sprite.image = assets.explosion.clone();
                sprite.texture_atlas = None;
                transform.rotation = Quat::IDENTITY;
            }
            sprite.custom_size = Some(Vec2::new(s as f32, s as f32));
        } else {
            sprite.custom_size = Some(Vec2::ZERO);
        }
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
                **text = format!(
                    "Angle: {:.2}  Power: {:.1}",
                    player.angle.to_degrees(),
                    player.power
                );
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

/// Get launch point using player data and transform (Bevy coords, center origin, Y-up).
/// `player.angle` is radians CCW from east.
pub fn get_launch_point_from_transform(player: &Player, transform: &Transform) -> (f64, f64) {
    compute_launch_point(
        transform.translation.x as f64,
        transform.translation.y as f64,
        player.gun_offset,
        player.angle,
    )
}
