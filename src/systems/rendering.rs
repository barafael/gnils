use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

/// Update bounce border animation.
pub fn update_bounce_animation(mut bounce: ResMut<BounceAnimation>) {
    bounce.count += bounce.inc;
    if bounce.count > 255.0 || bounce.count < 125.0 {
        bounce.inc *= -1.0;
        bounce.count += 2.0 * bounce.inc;
    }
}

/// Draw bounce border using gizmos.
pub fn draw_bounce_border(
    mut gizmos: Gizmos,
    settings: Res<GameSettings>,
    bounce: Res<BounceAnimation>,
) {
    if !settings.bounce {
        return;
    }

    let r = bounce.count / 255.0;
    let color = Color::srgb(r, 0.0, 0.0);
    let half_w = 400.0;
    let half_h = 300.0;

    gizmos.line_2d(Vec2::new(-half_w, -half_h), Vec2::new(half_w, -half_h), color);
    gizmos.line_2d(Vec2::new(half_w, -half_h), Vec2::new(half_w, half_h), color);
    gizmos.line_2d(Vec2::new(half_w, half_h), Vec2::new(-half_w, half_h), color);
    gizmos.line_2d(Vec2::new(-half_w, half_h), Vec2::new(-half_w, -half_h), color);
}

/// Hide/show angle-power UI based on game state.
pub fn update_ui_visibility(
    turn: Res<TurnState>,
    menu: Res<MenuOpen>,
    mut angle_power: Query<
        &mut Visibility,
        (
            With<UiAnglePower>,
            Without<UiMissileStatus>,
            Without<UiDimOverlay>,
            Without<UiRoundOverlay>,
            Without<UiEndRoundMsg>,
        ),
    >,
    mut missile_status: Query<
        &mut Visibility,
        (
            With<UiMissileStatus>,
            Without<UiAnglePower>,
            Without<UiDimOverlay>,
            Without<UiRoundOverlay>,
            Without<UiEndRoundMsg>,
        ),
    >,
) {
    for mut vis in angle_power.iter_mut() {
        *vis = if turn.firing || turn.round_over || menu.open {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
    }

    for mut vis in missile_status.iter_mut() {
        *vis = if turn.firing && !menu.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Handle invisible planets mode: hide during play, fade in on round over.
pub fn update_planet_visibility(
    mut turn: ResMut<TurnState>,
    settings: Res<GameSettings>,
    mut planets: Query<(&mut Sprite, &Planet)>,
) {
    if !settings.invisible {
        if settings.is_changed() {
            for (mut sprite, _) in planets.iter_mut() {
                sprite.color = Color::WHITE;
            }
        }
        return;
    }

    if turn.round_over {
        if turn.show_planets > 0.0 {
            let alpha = (255.0 - turn.show_planets * 2.55) / 255.0;
            for (mut sprite, planet) in planets.iter_mut() {
                if !planet.is_blackhole {
                    sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0) as f32);
                }
            }
            turn.show_planets -= 0.5;
        } else {
            for (mut sprite, _) in planets.iter_mut() {
                sprite.color = Color::WHITE;
            }
        }
    } else {
        for (mut sprite, _) in planets.iter_mut() {
            sprite.color = Color::srgba(1.0, 1.0, 1.0, 0.0);
        }
    }
}

/// Draw a zoom/minimap view when the missile is off-screen during firing.
/// A full-screen dim sprite (ZoomDimSprite) is toggled, and gizmos draw the overlay borders.
pub fn draw_zoom_view(
    mut gizmos: Gizmos,
    turn: Res<TurnState>,
    missile_q: Query<(&GravityBody, &MissileMarker)>,
    players: Query<(&Player, &Transform), Without<ShipBlendSprite>>,
    mut dim_q: Query<&mut Visibility, With<ZoomDimSprite>>,
) {
    let mut missile_pos = None;
    let mut missile_on_screen = true;
    if turn.firing {
        for (body, marker) in missile_q.iter() {
            if !marker.active {
                continue;
            }
            missile_on_screen = is_on_screen(body.pos);
            missile_pos = Some(body.pos);
        }
    }

    let show_zoom = turn.firing && !missile_on_screen && missile_pos.is_some();

    for mut vis in dim_q.iter_mut() {
        *vis = if show_zoom { Visibility::Visible } else { Visibility::Hidden };
    }

    if !show_zoom {
        return;
    }
    let mpos = missile_pos.unwrap();

    let zoom_w = 600.0_f32;
    let zoom_h = 450.0_f32;
    let game_w = 200.0_f32;
    let game_h = 150.0_f32;

    // White outer border
    let hw = zoom_w / 2.0;
    let hh = zoom_h / 2.0;
    let white = Color::WHITE;
    gizmos.line_2d(Vec2::new(-hw, hh), Vec2::new(hw, hh), white);
    gizmos.line_2d(Vec2::new(hw, hh), Vec2::new(hw, -hh), white);
    gizmos.line_2d(Vec2::new(hw, -hh), Vec2::new(-hw, -hh), white);
    gizmos.line_2d(Vec2::new(-hw, -hh), Vec2::new(-hw, hh), white);

    // Grey inner border (1/4 scale game viewport)
    let ghw = game_w / 2.0;
    let ghh = game_h / 2.0;
    let grey = Color::srgb(150.0 / 255.0, 150.0 / 255.0, 150.0 / 255.0);
    gizmos.line_2d(Vec2::new(-ghw, ghh), Vec2::new(ghw, ghh), grey);
    gizmos.line_2d(Vec2::new(ghw, ghh), Vec2::new(ghw, -ghh), grey);
    gizmos.line_2d(Vec2::new(ghw, -ghh), Vec2::new(-ghw, -ghh), grey);
    gizmos.line_2d(Vec2::new(-ghw, -ghh), Vec2::new(-ghw, ghh), grey);

    // Missile dot
    let mx = (mpos.0 as f32 / 800.0 - 0.5) * game_w;
    let my = (0.5 - mpos.1 as f32 / 600.0) * game_h;
    gizmos.circle_2d(Vec2::new(mx, my), 3.0, Color::srgb(1.0, 0.3, 0.3));

    // Player dots
    for (player, transform) in players.iter() {
        let px = (transform.translation.x / 800.0) * game_w;
        let py = (transform.translation.y / 600.0) * game_h;
        gizmos.circle_2d(Vec2::new(px, py), 2.0, player.color());
    }
}

/// Animate the "Round N" / "Game Over" overlay text with zoom and fade effect.
pub fn update_round_overlay(
    mut turn: ResMut<TurnState>,
    mut container_q: Query<
        (&mut Visibility, &Children),
        (
            With<UiRoundOverlay>,
            Without<UiDimOverlay>,
            Without<UiEndRoundMsg>,
        ),
    >,
    mut text_q: Query<(&mut Text, &mut TextFont, &mut TextColor), Without<UiRoundOverlay>>,
) {
    let show = turn.show_round > 30.0;

    for (mut vis, children) in container_q.iter_mut() {
        if show {
            *vis = Visibility::Visible;

            for &child in children {
                if let Ok((mut text, mut font, mut color)) = text_q.get_mut(child) {
                    if turn.game_over {
                        **text = "Game Over".to_string();
                    } else {
                        **text = format!("Round {}", turn.round);
                    }

                    let alpha = ((2.0 * turn.show_round - 60.0) / 255.0).clamp(0.0, 1.0);
                    color.0 = Color::srgba(1.0, 1.0, 1.0, alpha as f32);

                    let scale_factor = if turn.game_over { 15.0 } else { 25.0 };
                    let size = ((100.0 - turn.show_round) * 48.0 / scale_factor).max(4.0);
                    font.font_size = size as f32;
                }
            }
        } else {
            *vis = Visibility::Hidden;
        }
    }

    if show {
        turn.show_round /= 1.02;
    }
}

/// Show/hide the end-round message box during round_over.
pub fn update_round_over_display(
    turn: Res<TurnState>,
    round_result: Res<RoundResult>,
    mut container_q: Query<
        &mut Visibility,
        (
            With<UiEndRoundMsg>,
            Without<UiDimOverlay>,
            Without<UiRoundOverlay>,
            Without<UiAnglePower>,
            Without<UiMissileStatus>,
        ),
    >,
    mut text_q: Query<
        &mut Text,
        (
            With<UiDimOverlay>,
            Without<UiEndRoundMsg>,
            Without<UiRoundOverlay>,
            Without<UiAnglePower>,
            Without<UiMissileStatus>,
        ),
    >,
    players: Query<&Player, Without<ShipBlendSprite>>,
) {
    let show_msg = turn.round_over && turn.show_round <= 30.0 && turn.show_planets <= 0.0;

    for mut vis in container_q.iter_mut() {
        *vis = if show_msg && round_result.hit_player > 0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    if show_msg && round_result.hit_player > 0 && (round_result.is_changed() || turn.is_changed()) {
        for mut text in text_q.iter_mut() {
            let mut lines = Vec::new();
            lines.push(round_result.message.clone());
            lines.push(String::new());

            if round_result.self_hit {
                lines.push(format!("Hit self:              {}", round_result.hit_score));
            } else {
                lines.push(format!("Hit opponent:        {}", round_result.hit_score));
                if round_result.quick_bonus > 0 {
                    lines.push(format!("Quickhit bonus:    {}", round_result.quick_bonus));
                }
                if round_result.power_penalty != 0 {
                    lines.push(format!("Power penalty:     {}", round_result.power_penalty));
                }
            }

            lines.push(String::new());
            lines.push(format!("{} added to score", round_result.total_score));

            if turn.game_over {
                lines.push(String::new());
                let mut p1_score = 0;
                let mut p2_score = 0;
                for player in players.iter() {
                    if player.id == 1 { p1_score = player.score; }
                    else { p2_score = player.score; }
                }
                if p1_score > p2_score {
                    lines.push("Player 1 has won the game".to_string());
                } else if p2_score > p1_score {
                    lines.push("Player 2 has won the game".to_string());
                } else {
                    lines.push("The game has ended in a tie".to_string());
                }
            }

            lines.push(String::new());
            lines.push(if turn.game_over {
                "Press fire for a new game".to_string()
            } else {
                "Press fire for a new round".to_string()
            });

            **text = lines.join("\n");
        }
    }
}

/// Show the settings menu overlay and update its text content.
/// Text is only rebuilt when menu state or settings actually change.
pub fn update_menu_display(
    settings: Res<GameSettings>,
    menu: Res<MenuOpen>,
    mut vis_q: Query<&mut Visibility, With<UiMenuOverlay>>,
    mut text_q: Query<&mut Text, With<crate::components::UiMenuText>>,
) {
    for mut vis in vis_q.iter_mut() {
        *vis = if menu.open { Visibility::Visible } else { Visibility::Hidden };
    }

    if !menu.open {
        return;
    }

    // Only rebuild text when something actually changed
    if !menu.is_changed() && !settings.is_changed() {
        return;
    }

    // Display rows: (label, value_string). Empty label = separator line.
    let rows: &[(&str, Option<String>)] = &[
        ("Resume Game", None),
        ("New Game", None),
        ("", None),
        ("Bounce", Some(bool_str(settings.bounce))),
        ("Fixed Power", Some(bool_str(settings.fixed_power))),
        ("Invisible Planets", Some(bool_str(settings.invisible))),
        ("Particles", Some(bool_str(settings.particles_enabled))),
        ("", None),
        ("Max Planets", Some(settings.max_planets.to_string())),
        ("Max Blackholes", Some(settings.max_blackholes.to_string())),
        ("Rounds", Some(if settings.max_rounds == 0 { "∞".into() } else { settings.max_rounds.to_string() })),
        ("", None),
        ("Fullscreen", Some(bool_str(settings.fullscreen))),
    ];

    // Map menu.selected index to display row index (skip separators)
    let selectable: Vec<usize> = rows.iter().enumerate()
        .filter(|(_, (label, _))| !label.is_empty())
        .map(|(i, _)| i)
        .collect();
    let selected_row = selectable.get(menu.selected).copied().unwrap_or(0);

    let mut lines = vec![
        "  ── SETTINGS ──".to_string(),
        String::new(),
    ];
    for (i, (label, value)) in rows.iter().enumerate() {
        if label.is_empty() {
            lines.push(String::new());
            continue;
        }
        let arrow = if i == selected_row { "►" } else { "  " };
        match value {
            None => lines.push(format!("{}  {}", arrow, label)),
            Some(v) => lines.push(format!("{}  {:<22}{}", arrow, label, v)),
        }
    }
    lines.push(String::new());
    lines.push("  Up/Down: navigate   Enter/Left/Right: change   Esc: close".to_string());

    for mut text in text_q.iter_mut() {
        **text = lines.join("\n");
    }
}

fn bool_str(v: bool) -> String {
    if v { "ON".into() } else { "OFF".into() }
}
