use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

/// Update player ship transforms based on position and rotation.
/// Selects atlas frames using ship_blend::compute_blend_frames and updates
/// a companion ShipBlendSprite for smooth inter-frame blending.
pub fn update_player_sprites(
    mut players: Query<(&Player, &mut Transform, &mut Sprite), Without<ShipBlendSprite>>,
    mut blend_sprites: Query<(&ShipBlendSprite, &mut Transform, &mut Sprite), Without<Player>>,
    assets: Res<GameAssets>,
) {
    for (player, mut transform, mut sprite) in players.iter_mut() {
        if player.shot {
            // Hide companion blend sprite during explosion
            for (blend, _, mut b_sprite) in blend_sprites.iter_mut() {
                if blend.player_id == player.id {
                    b_sprite.color = Color::NONE;
                }
            }
            continue;
        }

        // Restore ship sprite if it was replaced by explosion
        if sprite.texture_atlas.is_none() {
            let ship_image = if player.id == 1 {
                assets.red_ship.clone()
            } else {
                assets.blue_ship.clone()
            };
            sprite.image = ship_image;
            sprite.texture_atlas = Some(TextureAtlas {
                layout: assets.ship_atlas_layout.clone(),
                index: 0,
            });
            sprite.custom_size = None;
        }

        // Compute blend frames: img1 = primary frame, img2 = secondary, f = blend factor
        let (img1, img2, blend_f) = crate::ship_blend::compute_blend_frames(player.rel_rot);

        if let Some(ref mut atlas) = sprite.texture_atlas {
            atlas.index = img1;
        }
        sprite.color = Color::WHITE;

        // Full rotation matching Python's rotozoom(-rel_rot, 1.0)
        transform.rotation = Quat::from_rotation_z(-(player.rel_rot as f32).to_radians());

        // Update blend sprite: mirror position/rotation, different frame, partial alpha
        for (blend, mut b_transform, mut b_sprite) in blend_sprites.iter_mut() {
            if blend.player_id != player.id {
                continue;
            }

            // Restore blend sprite image if needed (e.g. after explosion)
            if b_sprite.texture_atlas.is_none() {
                let ship_image = if player.id == 1 {
                    assets.red_ship.clone()
                } else {
                    assets.blue_ship.clone()
                };
                b_sprite.image = ship_image;
                b_sprite.texture_atlas = Some(TextureAtlas {
                    layout: assets.ship_atlas_layout.clone(),
                    index: img2,
                });
                b_sprite.custom_size = None;
            }

            if let Some(ref mut atlas) = b_sprite.texture_atlas {
                atlas.index = img2;
            }
            b_sprite.color = Color::srgba(1.0, 1.0, 1.0, blend_f as f32);
            b_transform.translation = transform.translation + Vec3::Z * 0.05;
            b_transform.rotation = transform.rotation;
        }
    }
}

/// Draw the aiming line using gizmos.
pub fn draw_aim_line(
    mut gizmos: Gizmos,
    players: Query<(&Player, &Transform), Without<ShipBlendSprite>>,
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
        let end_x = lx + player.power * player.angle.to_radians().sin();
        let end_y = ly - player.power * player.angle.to_radians().cos();

        let start_bevy = pygame_to_bevy(lx, ly);
        let end_bevy = pygame_to_bevy(end_x, end_y);

        gizmos.line_2d(start_bevy, end_bevy, player.color());
    }
}

/// Update ship explosion animation for hit players.
/// Uses half-frame increments since this runs at ~60fps but Python runs at 30fps.
pub fn update_ship_explosion(
    mut players: Query<(&mut Player, &mut Sprite, &mut Transform), Without<ShipBlendSprite>>,
    mut blend_sprites: Query<(&ShipBlendSprite, &mut Sprite), Without<Player>>,
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
            if sprite.texture_atlas.is_some() {
                sprite.image = assets.explosion.clone();
                sprite.texture_atlas = None;
                transform.rotation = Quat::IDENTITY;
            }
            sprite.custom_size = Some(Vec2::new(s as f32, s as f32));
        } else {
            sprite.custom_size = Some(Vec2::ZERO);
        }

        // Hide blend sprite during explosion
        for (blend, mut b_sprite) in blend_sprites.iter_mut() {
            if blend.player_id == player.id {
                b_sprite.color = Color::NONE;
            }
        }
    }
}

/// Update UI text for scores and angle/power.
pub fn update_ui_text(
    players: Query<&Player, Without<ShipBlendSprite>>,
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
