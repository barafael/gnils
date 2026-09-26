use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use gnils_protocol::compute_launch_point;

use crate::components::*;
use crate::resources::*;
use crate::ship_blend;
use crate::systems::round::scores;

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

        // Which two frames to interpolate, and how far between them.
        // `rel_rot` is in radians; `compute_blend_frames` expects degrees.
        let (img1, img2, blend_f) = ship_blend::compute_blend_frames(player.rel_rot.to_degrees());

        let strip_handle = if player.id == 1 {
            &assets.red_ship
        } else {
            &assets.blue_ship
        };

        // Extract the two frames (releases borrow on images)
        let frames = images.get(strip_handle).map(|strip| {
            (
                ship_blend::extract_frame(strip, img1),
                ship_blend::extract_frame(strip, img2),
            )
        });

        if let Some((frame1, frame2)) = frames
            && let Some(mut target) = images.get_mut(blended_handle)
        {
            *target = ship_blend::blend_frames(&frame1, &frame2, blend_f);
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
    menu: Res<MenuOpen>,
) {
    if turn.firing || turn.round_over || menu.open {
        return;
    }

    for (player, transform) in players.iter() {
        if player.id != turn.current_player {
            continue;
        }
        let (lx, ly) = launch_point(player, transform);
        let end = Vec2::new(
            (lx + player.power * player.angle.cos()) as f32,
            (ly + player.power * player.angle.sin()) as f32,
        );
        gizmos.line_2d(Vec2::new(lx as f32, ly as f32), end, player.color());
    }
}

/// Update ship explosion animation for hit players.
/// Scaled by delta time to match the original 30fps cadence at any framerate.
pub fn update_ship_explosion(
    time: Res<Time>,
    mut players: Query<(&mut Player, &mut Sprite, &mut Transform)>,
    assets: Res<GameAssets>,
) {
    for (mut player, mut sprite, mut transform) in players.iter_mut() {
        if !player.shot {
            continue;
        }

        let just_started = player.explosion_progress == 0.0;
        player.explosion_progress += time.delta_secs_f64() * 30.0;

        // The fireball swells and shrinks again over six frames.
        let e = player.explosion_progress;
        let s = e * (6.0 - e) * 100.0 / 9.0;

        if s <= 0.0 {
            sprite.custom_size = Some(Vec2::ZERO);
            continue;
        }
        if just_started {
            sprite.image = assets.explosion.clone();
            sprite.texture_atlas = None;
            transform.rotation = Quat::IDENTITY;
        }
        sprite.custom_size = Some(Vec2::splat(s as f32));
    }
}

/// One HUD line's text. Bevy hands out several `&mut Text` at once only when
/// the queries are provably disjoint, so each line excludes the other three.
type HudLine<'w, 's, Mine, A, B, C> =
    Query<'w, 's, &'static mut Text, (With<Mine>, Without<A>, Without<B>, Without<C>)>;

/// The four HUD text lines, bundled to keep their disjointness filters out of
/// the system signature.
#[derive(SystemParam)]
pub struct HudTexts<'w, 's> {
    score_p1: HudLine<'w, 's, UiScoreP1, UiScoreP2, UiAnglePower, UiRoundInfo>,
    score_p2: HudLine<'w, 's, UiScoreP2, UiScoreP1, UiAnglePower, UiRoundInfo>,
    angle_power: HudLine<'w, 's, UiAnglePower, UiScoreP1, UiScoreP2, UiRoundInfo>,
    round_info: HudLine<'w, 's, UiRoundInfo, UiScoreP1, UiScoreP2, UiAnglePower>,
}

/// Update UI text for scores, angle/power and the round counter.
pub fn update_ui_text(
    players: Query<&Player>,
    turn: Res<TurnState>,
    settings: Res<GameSettings>,
    net: Res<gnils_net::NetState>,
    mut hud: HudTexts,
) {
    let (s1, s2) = scores(players.iter());
    if let Ok(mut text) = hud.score_p1.single_mut() {
        **text = format!("{}  --  {s1}", net.player_name(1).unwrap_or("Player 1"));
    }
    if let Ok(mut text) = hud.score_p2.single_mut() {
        **text = format!("{s2}  --  {}", net.player_name(2).unwrap_or("Player 2"));
    }

    if !turn.firing
        && !turn.round_over
        && let Some(player) = players.iter().find(|p| p.id == turn.current_player)
        && let Ok(mut text) = hud.angle_power.single_mut()
    {
        **text = format!(
            "Angle: {:.2}  Power: {:.1}",
            player.angle.to_degrees(),
            player.power
        );
    }

    if let Ok(mut text) = hud.round_info.single_mut() {
        **text = if settings.max_rounds > 0 {
            format!("Round {} of {}", turn.round, settings.max_rounds)
        } else {
            format!("Round {}", turn.round)
        };
    }
}

/// Network games: say whose turn it is, so a silently ignoring keyboard is
/// not a mystery. Hidden in local play — on one machine it is obvious.
pub fn update_turn_banner(
    turn: Res<TurnState>,
    net_mode: Res<NetworkMode>,
    net: Res<gnils_net::NetState>,
    phase: Res<State<GamePhase>>,
    menu: Res<MenuOpen>,
    mut q: Query<(&mut Text, &mut Visibility), With<UiTurnBanner>>,
) {
    let show = net_mode.is_network()
        && *phase.get() == GamePhase::Aiming
        && !turn.round_over
        && !menu.open;
    for (mut text, mut vis) in q.iter_mut() {
        *vis = visibility(show);
        if !show {
            continue;
        }
        **text = if Some(turn.current_player) == net_mode.player_id() {
            "Your turn".to_string()
        } else {
            format!(
                "{} is aiming...",
                net.player_name(turn.current_player).unwrap_or("Opponent")
            )
        };
    }
}

/// Where this player's shot leaves the gun (Bevy coords, center origin,
/// Y-up). `player.angle` is radians CCW from east.
pub fn launch_point(player: &Player, transform: &Transform) -> (f64, f64) {
    compute_launch_point(
        transform.translation.x as f64,
        transform.translation.y as f64,
        player.gun_offset,
        player.angle,
    )
}
