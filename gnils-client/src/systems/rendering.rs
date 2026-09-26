use bevy::prelude::*;
use bevy::ui::UiScale;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;
use crate::systems::round::scores;

/// Scale HUD/menu layout with the window height, so text and panels keep the
/// same proportion to the playfield at any window size (the world itself
/// scales through the camera's `AutoMin` mode).
pub fn update_ui_scale(windows: Query<&Window, Changed<Window>>, mut ui_scale: ResMut<UiScale>) {
    let Ok(window) = windows.single() else {
        return;
    };
    if window.height() <= 0.0 {
        return;
    }
    let scale = (window.height() / WINDOW_HEIGHT).max(0.5);
    if (ui_scale.0 - scale).abs() > f32::EPSILON {
        ui_scale.0 = scale;
    }
}

/// The original ran at a fixed 30 fps and stepped its animations once per
/// frame. Everything timed against that cadence scales by `dt * this`, so it
/// runs at the original speed whatever the display does.
const FRAMES_PER_SEC: f64 = 30.0;

/// What the original divided `show_round` by on each of those frames.
const ROUND_OVERLAY_DECAY: f64 = 1.04;

/// Update bounce border animation.
pub fn update_bounce_animation(time: Res<Time>, mut bounce: ResMut<BounceAnimation>) {
    let dt30 = time.delta_secs() * 30.0;
    bounce.count += bounce.inc * dt30;
    if bounce.count > 255.0 || bounce.count < 125.0 {
        bounce.inc *= -1.0;
        bounce.count += 2.0 * bounce.inc * dt30;
    }
}

/// Draw the pulsing red frame that marks the walls in bounce mode.
pub fn draw_bounce_border(
    mut gizmos: Gizmos,
    settings: Res<GameSettings>,
    bounce: Res<BounceAnimation>,
) {
    if settings.bounce {
        let color = Color::srgb(bounce.count / 255.0, 0.0, 0.0);
        gizmos.rect_2d(
            Isometry2d::IDENTITY,
            Vec2::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            color,
        );
    }
}

/// Show each HUD line when the original showed it. Angle and power belong
/// to a player who is still aiming; the shot's readouts replace them while
/// it is in the air, and the round counter gives up the bottom row to the
/// timeout for the same stretch.
pub fn update_ui_visibility(
    turn: Res<TurnState>,
    menu: Res<MenuOpen>,
    mut hud: Query<(&mut Visibility, &HudSlot)>,
) {
    let aiming = !turn.firing && !turn.round_over && !menu.open;
    let in_flight = turn.firing && !menu.open;
    for (mut vis, slot) in hud.iter_mut() {
        let shown = match slot {
            HudSlot::Angle | HudSlot::Power => aiming,
            HudSlot::PowerPenalty | HudSlot::Timeout => in_flight,
            HudSlot::RoundInfo => !turn.firing,
            // The scoreboard always stands; the banner has its own system.
            HudSlot::ScoreP1 | HudSlot::ScoreP2 | HudSlot::TurnBanner => continue,
        };
        *vis = visibility(shown);
    }
}

/// Handle invisible planets mode: hide during play, fade in on round over.
pub fn update_planet_visibility(
    time: Res<Time>,
    mut turn: ResMut<TurnState>,
    settings: Res<GameSettings>,
    mut planets: Query<(&mut Sprite, &Planet)>,
) {
    if !settings.invisible {
        // Nothing to hide — but a mid-game switch has to undo the last fade.
        if settings.is_changed() {
            paint_planets(&mut planets, 1.0, false);
        }
        return;
    }

    match (turn.round_over, turn.show_planets > 0.0) {
        // Fading the planets back in, a little further each frame.
        (true, true) => {
            let alpha = ((255.0 - turn.show_planets * 2.55) / 255.0).clamp(0.0, 1.0);
            paint_planets(&mut planets, alpha as f32, true);
            turn.show_planets -= time.delta_secs_f64() * FRAMES_PER_SEC;
        }
        (true, false) => paint_planets(&mut planets, 1.0, false),
        (false, _) => paint_planets(&mut planets, 0.0, false),
    }
}

/// Set every planet's sprite alpha. `planets_only` leaves blackholes alone —
/// they are invisible by nature and never fade in.
fn paint_planets(planets: &mut Query<(&mut Sprite, &Planet)>, alpha: f32, planets_only: bool) {
    for (mut sprite, planet) in planets.iter_mut() {
        if planets_only && planet.is_blackhole {
            continue;
        }
        sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
    }
}

/// Visible world size implied by the camera's `AutoMin` scaling mode:
/// the 800x600 playfield is always fully shown, and the wider/taller
/// axis grows with the window aspect ratio.
pub fn visible_world_size(window: &Window) -> Vec2 {
    let (w, h) = (window.width(), window.height());
    if w * WINDOW_HEIGHT > WINDOW_WIDTH * h {
        Vec2::new(w * WINDOW_HEIGHT / h, WINDOW_HEIGHT)
    } else {
        Vec2::new(WINDOW_WIDTH, h * WINDOW_WIDTH / w)
    }
}

/// Stretch the backdrop and zoom-dim sprites so they cover the whole
/// visible camera area, which changes with the window aspect ratio.
#[allow(clippy::type_complexity)]
pub fn update_view_size(
    windows: Query<&Window, Changed<Window>>,
    mut cover_q: Query<&mut Sprite, Or<(With<Backdrop>, With<ZoomDimSprite>)>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if window.width() <= 0.0 || window.height() <= 0.0 {
        return;
    }
    let size = visible_world_size(window);
    for mut sprite in cover_q.iter_mut() {
        if sprite.custom_size != Some(size) {
            sprite.custom_size = Some(size);
        }
    }
}

/// Grow the trail canvas to cover everything the camera shows.
///
/// The playfield is 4:3 but the window need not be, and the camera reveals
/// more world on the long axis. A trail line drawn past the canvas edge is
/// clipped, so a canvas fixed at 4:3 leaves the shot's trace stopping in
/// mid-air. Checked every frame rather than on `Changed<Window>` so a canvas
/// that somehow falls out of step corrects itself.
pub fn resize_trail_canvas(
    windows: Query<&Window>,
    mut trail: ResMut<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut sprite_q: Query<&mut Sprite, With<TrailSprite>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if window.width() <= 0.0 || window.height() <= 0.0 {
        return;
    }
    let size = visible_world_size(window).ceil().as_uvec2();
    if size == trail.size {
        return;
    }
    let Some(mut image) = images.get_mut(&trail.image_handle) else {
        return;
    };
    // Resizing starts the trail over; it only happens when the window shape
    // changes, and a round begins with a cleared canvas anyway.
    *image = crate::systems::setup::blank_image(size.x, size.y);
    trail.size = size;

    // Say the size on the sprite rather than leaning on its fall-back to the
    // texture's: that fall-back leaves the culling bounds stale, since they
    // are only recomputed when the sprite itself changes.
    for mut sprite in sprite_q.iter_mut() {
        sprite.custom_size = Some(size.as_vec2());
    }
}

/// Draw a zoom/minimap view when the missile is off-screen during firing.
/// A full-screen dim sprite (ZoomDimSprite) is toggled, and gizmos draw the
/// overlay borders.
pub fn draw_zoom_view(
    mut gizmos: Gizmos,
    turn: Res<TurnState>,
    missile_q: Query<(&GravityBody, &MissileMarker)>,
    players: Query<(&Player, &Transform)>,
    proj_q: Query<&Projection, With<Camera2d>>,
    mut dim_q: Query<&mut Visibility, With<ZoomDimSprite>>,
) {
    let Ok(Projection::Orthographic(proj)) = proj_q.single() else {
        return;
    };

    // The minimap only appears while an active missile is out of view.
    let off_screen = turn.firing.then(|| {
        missile_q
            .iter()
            .filter(|(_, marker)| marker.active)
            .map(|(body, _)| body.pos)
            .find(|pos| !proj.area.contains(Vec2::new(pos.0 as f32, pos.1 as f32)))
    });
    let missile_pos = off_screen.flatten();

    for mut vis in dim_q.iter_mut() {
        *vis = visibility(missile_pos.is_some());
    }
    let Some(mpos) = missile_pos else {
        return;
    };

    // A white frame for the minimap, and inside it a grey one at 1/4 scale
    // standing for the game viewport.
    const ZOOM: Vec2 = Vec2::new(600.0, 450.0);
    const GAME: Vec2 = Vec2::new(200.0, 150.0);
    gizmos.rect_2d(Isometry2d::IDENTITY, ZOOM, Color::WHITE);
    gizmos.rect_2d(Isometry2d::IDENTITY, GAME, rgb((150, 150, 150)));

    // Everything on the minimap sits at the same 1/4 scale.
    let scaled = |x: f32, y: f32| Vec2::new(x / WINDOW_WIDTH, y / WINDOW_HEIGHT) * GAME;
    gizmos.circle_2d(
        scaled(mpos.0 as f32, mpos.1 as f32),
        3.0,
        Color::srgb(1.0, 0.3, 0.3),
    );
    for (player, transform) in players.iter() {
        let p = transform.translation;
        gizmos.circle_2d(scaled(p.x, p.y), 2.0, player.color());
    }
}

/// Animate the "Round N" / "Game Over" overlay text with zoom and fade effect.
///
/// The text query is only accessed via `get_mut` on the overlay's children,
/// so no disjointness filters are needed.
#[allow(clippy::type_complexity)]
pub fn update_round_overlay(
    time: Res<Time>,
    mut turn: ResMut<TurnState>,
    mut container_q: Query<(&mut Visibility, &Children), With<UiRoundOverlay>>,
    mut text_q: Query<(&mut Text, &mut TextFont, &mut TextColor)>,
) {
    let show = turn.show_round > 30.0;

    for (mut vis, children) in container_q.iter_mut() {
        *vis = visibility(show);
        if !show {
            continue;
        }
        for &child in children {
            let Ok((mut text, mut font, mut color)) = text_q.get_mut(child) else {
                continue;
            };
            **text = if turn.game_over {
                "Game Over".to_string()
            } else {
                format!("Round {}", turn.round)
            };

            let alpha = ((2.0 * turn.show_round - 60.0) / 255.0).clamp(0.0, 1.0);
            color.0 = Color::srgba(1.0, 1.0, 1.0, alpha as f32);

            // The text starts tiny and swells as `show_round` decays.
            let scale_factor = if turn.game_over { 15.0 } else { 25.0 };
            let size = ((100.0 - turn.show_round) * 48.0 / scale_factor).max(4.0);
            font.font_size = FontSize::Px(size as f32);
        }
    }

    if show {
        turn.show_round /= ROUND_OVERLAY_DECAY.powf(time.delta_secs_f64() * FRAMES_PER_SEC);
    }
}

/// Show the end-of-round panel and lay it out the way the original did:
/// a heading when the game is over, who shot whom, the score breakdown with
/// its values flush right, what it came to, and how to carry on.
///
/// The rows are rebuilt once per result, not per frame — the last line
/// carries a live countdown in network games and is written in place.
#[allow(clippy::too_many_arguments)]
pub fn update_round_over_display(
    turn: Res<TurnState>,
    round_result: Res<RoundResult>,
    net_mode: Res<NetworkMode>,
    advance: Res<RoundAdvance>,
    players: Query<&Player>,
    net: Res<gnils_net::NetState>,
    mut container_q: Query<&mut Visibility, With<UiEndRoundMsg>>,
    column_q: Query<Entity, With<UiEndRoundColumn>>,
    row_q: Query<Entity, With<UiEndRoundRow>>,
    mut prompt_q: Query<&mut Text, With<UiEndRoundPrompt>>,
    mut commands: Commands,
) {
    let show_msg = turn.round_over
        && turn.show_round <= 30.0
        && turn.show_planets <= 0.0
        && round_result.hit_player > 0;
    for mut vis in container_q.iter_mut() {
        *vis = visibility(show_msg);
    }

    // The countdown ticks every frame; only its line changes.
    let prompt = match (turn.game_over, net_mode.is_network()) {
        (true, true) => next_step_text(&advance, "New game starting in", "New game starting..."),
        (true, false) => "Press fire for a new game or escape for the menu".to_string(),
        (false, true) => next_step_text(&advance, "Next round in", "Next round starting..."),
        (false, false) => "Press fire for a new round or escape for the menu".to_string(),
    };
    for mut text in prompt_q.iter_mut() {
        if text.as_str() != prompt {
            **text = prompt.clone();
        }
    }

    if !round_result.is_changed() {
        return;
    }
    let Ok(column) = column_q.single() else {
        return;
    };
    for entity in row_q.iter() {
        commands.entity(entity).despawn();
    }

    let name_of = |id: u8| {
        net.player_name(id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("Player {id}"))
    };
    let (p1, p2) = scores(players.iter());
    let game_over = turn.game_over;
    let result = round_result.clone();

    commands.entity(column).with_children(|parent| {
        if game_over {
            parent.spawn((
                Text::new("Game over"),
                TextFont {
                    font_size: FontSize::Px(MENU_HEADING_SIZE),
                    ..default()
                },
                TextColor(Color::WHITE),
                UiEndRoundRow,
            ));
        }

        parent.spawn((
            Text::new(result.message.clone()),
            panel_font(),
            TextColor(Color::WHITE),
            UiEndRoundRow,
        ));

        // The score breakdown: label left, value flush right. A self-hit is
        // a flat deduction, so it has no bonus or penalty to show.
        let mut rows = vec![if result.self_hit {
            ("Hit self:", result.hit_score)
        } else {
            ("Hit opponent:", result.hit_score)
        }];
        if !result.self_hit {
            rows.push(("Quickhit bonus:", result.quick_bonus));
            rows.push(("Power penalty:", result.power_penalty));
        }
        for (label, value) in rows {
            parent
                .spawn((
                    Node {
                        width: Val::Px(crate::systems::setup::END_ROUND_WIDTH),
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        ..default()
                    },
                    UiEndRoundRow,
                ))
                .with_children(|row| {
                    row.spawn((Text::new(label), panel_font(), TextColor(Color::WHITE)));
                    row.spawn((
                        Text::new(value.to_string()),
                        panel_font(),
                        TextColor(Color::WHITE),
                    ));
                });
        }

        parent.spawn((
            Text::new(if result.self_hit {
                format!("{} deducted from score", -result.total_score)
            } else {
                format!("{} added to score", result.total_score)
            }),
            panel_font(),
            TextColor(Color::WHITE),
            UiEndRoundRow,
        ));

        if game_over {
            let winner = match p1.cmp(&p2) {
                std::cmp::Ordering::Greater => Some(1),
                std::cmp::Ordering::Less => Some(2),
                std::cmp::Ordering::Equal => None,
            };
            parent.spawn((
                Text::new(match winner {
                    Some(id) => format!("{} has won the game", name_of(id)),
                    None => "The game has ended in a tie".to_string(),
                }),
                panel_font(),
                TextColor(Color::WHITE),
                UiEndRoundRow,
            ));
        }

        parent.spawn((
            Text::new(prompt.clone()),
            panel_font(),
            TextColor(Color::WHITE),
            UiEndRoundRow,
            UiEndRoundPrompt,
        ));
    });
}

/// The panel's body text, at the original's HUD size.
fn panel_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(MENU_FONT_SIZE),
        ..default()
    }
}

/// The original set the panel's "Game over" heading in its menu font.
const MENU_HEADING_SIZE: f32 = 26.0;

/// The network auto-advance line: a countdown while one is running.
fn next_step_text(advance: &RoundAdvance, counting: &str, waiting: &str) -> String {
    match advance.0 {
        Some(secs) => format!("{counting} {}...", secs.ceil() as u32),
        None => format!("{waiting}..."),
    }
}

// ── In-game settings menu ──────────────────────────────────────────────────

/// Label column width of the settings menu, in layout units (scaled with the
/// window like the rest of the UI). Wide enough for "Invisible Planets".
const MENU_LABEL_WIDTH: f32 = 190.0;
const MENU_FONT_SIZE: f32 = 14.0;

fn menu_font() -> TextFont {
    TextFont {
        font_size: FontSize::Px(MENU_FONT_SIZE),
        ..default()
    }
}

/// Show the settings menu overlay and update its rows.
/// Rows are only rebuilt when menu state or settings actually change.
pub fn update_menu_display(
    settings: Res<GameSettings>,
    menu: Res<MenuOpen>,
    net_mode: Res<NetworkMode>,
    mut vis_q: Query<&mut Visibility, With<UiMenuOverlay>>,
    column_q: Query<Entity, With<UiMenuColumn>>,
    row_q: Query<Entity, With<UiMenuRow>>,
    mut commands: Commands,
) {
    for mut vis in vis_q.iter_mut() {
        *vis = visibility(menu.open);
    }

    // Only rebuild rows when something actually changed.
    if !menu.open || !(menu.is_changed() || settings.is_changed() || net_mode.is_changed()) {
        return;
    }

    let Ok(column) = column_q.single() else {
        return;
    };
    for entity in row_q.iter() {
        commands.entity(entity).despawn();
    }

    let selected = menu.item();

    commands.entity(column).with_children(|parent| {
        parent.spawn((
            Text::new("-- SETTINGS --"),
            menu_font(),
            TextColor(Color::WHITE),
            UiMenuRow,
        ));
        for item in MENU_ITEMS {
            let color = if item == selected {
                Color::srgb(1.0, 0.9, 0.3)
            } else {
                Color::srgb(0.8, 0.8, 0.8)
            };
            parent
                .spawn((two_col_row(), UiMenuRow))
                .with_children(|row| {
                    // Fixed-width columns: label flush right, value flush
                    // left, so every row's gutter lines up.
                    row.spawn(text_cell(
                        item.label(),
                        MENU_LABEL_WIDTH,
                        Justify::Right,
                        MENU_FONT_SIZE,
                        color,
                    ));
                    if let Some(v) = item.value(&settings) {
                        row.spawn(text_cell(&v, 70.0, Justify::Left, MENU_FONT_SIZE, color));
                    }
                });
        }
        parent.spawn((
            Text::new("Up/Down: navigate   Enter/Left/Right: change   Esc: close"),
            menu_font(),
            TextColor(Color::WHITE),
            UiMenuRow,
        ));
    });
}

/// The container of a two-column row: a label column and a value column,
/// side by side with a fixed gutter. Shared with the lobby's menus.
pub(crate) fn two_col_row() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(14.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::FlexStart,
        ..default()
    }
}

/// One fixed-width text cell of a two-column row.
pub(crate) fn text_cell(
    text: &str,
    width: f32,
    justify: Justify,
    size: f32,
    color: Color,
) -> impl Bundle {
    (
        Node {
            width: Val::Px(width),
            ..default()
        },
        Text::new(text.to_string()),
        TextLayout {
            justify,
            ..default()
        },
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}
