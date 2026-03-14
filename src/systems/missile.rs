use bevy::prelude::*;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;
use crate::systems::player::get_launch_point_from_transform;
use crate::trail;

/// Handle fire missile: launch the missile from the current player's gun.
/// Called when turn.firing is set to true by the input system.
pub fn fire_missile(
    mut missile_q: Query<(&mut GravityBody, &mut MissileMarker, &mut Visibility)>,
    mut players: Query<(&mut Player, &Transform)>,
    mut turn: ResMut<TurnState>,
    settings: Res<GameSettings>,
) {
    if !turn.firing || turn.round_over {
        return;
    }

    // Check if missile is already active (already launched)
    for (_, marker, _) in missile_q.iter() {
        if marker.active {
            return;
        }
    }

    let current = turn.current_player;
    if current == 0 {
        return; // No player active
    }

    let mut launch_pos = (0.0, 0.0);
    let mut speed = 0.0;
    let mut angle_rad = 0.0;
    let mut trail_color = PLAYER1_COLOR;
    let mut power_penalty = 0;

    for (mut player, transform) in players.iter_mut() {
        if player.id != current {
            continue;
        }
        launch_pos = get_launch_point_from_transform(&player, transform);
        speed = player.power;
        angle_rad = player.angle.to_radians();
        trail_color = player.color_rgb;
        power_penalty = -(PENALTY_FACTOR * speed) as i32;
        player.attempts += 1;
    }

    for (mut body, mut marker, mut vis) in missile_q.iter_mut() {
        body.pos = launch_pos;
        body.last_pos = launch_pos;
        body.velocity = (
            0.1 * speed * angle_rad.sin(),
            0.1 * speed * angle_rad.cos(),
        );
        body.flight = settings.max_flight;

        marker.active = true;
        marker.trail_color = trail_color;
        marker.power_penalty = power_penalty;

        *vis = Visibility::Visible;
    }

    info!(
        "Player {} fires: pos=({:.1},{:.1}) vel=({:.2},{:.2}) power={:.1}",
        current,
        launch_pos.0,
        launch_pos.1,
        0.1 * speed * angle_rad.sin(),
        0.1 * speed * angle_rad.cos(),
        speed
    );
    turn.last_player = current;
    turn.current_player = 0; // no player active while firing
}

/// Draw the missile trail on the trail canvas.
pub fn draw_missile_trail(
    missile_q: Query<(&GravityBody, &MissileMarker)>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    turn: Res<TurnState>,
) {
    if !turn.firing {
        return;
    }

    for (body, marker) in missile_q.iter() {
        if !marker.active {
            continue;
        }

        if let Some(image) = images.get_mut(&trail_canvas.image_handle) {
            // Trail canvas is a pixel buffer (0..800, 0..600, Y-down).
            // Convert from Bevy coords (center origin, Y-up) to pixel coords.
            trail::draw_aa_line(
                image,
                body.last_pos.0 + 400.0,
                300.0 - body.last_pos.1,
                body.pos.0 + 400.0,
                300.0 - body.pos.1,
                marker.trail_color,
            );
        }
    }
}

/// Update missile visibility based on whether it's on screen.
pub fn update_missile_visibility(
    mut missile_q: Query<(&GravityBody, &MissileMarker, &mut Visibility)>,
    turn: Res<TurnState>,
) {
    if !turn.firing {
        for (_, _, mut vis) in missile_q.iter_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    }

    for (body, marker, mut vis) in missile_q.iter_mut() {
        if !marker.active {
            *vis = Visibility::Hidden;
            continue;
        }
        let on_screen = is_on_screen(body.pos);
        *vis = if on_screen {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Update missile status UI text.
pub fn update_missile_ui(
    missile_q: Query<(&GravityBody, &MissileMarker)>,
    turn: Res<TurnState>,
    mut status_q: Query<
        (&mut Text, &mut Visibility),
        (With<UiMissileStatus>, Without<MissileMarker>),
    >,
) {
    for (mut text, mut vis) in status_q.iter_mut() {
        if !turn.firing {
            *vis = Visibility::Hidden;
            continue;
        }

        *vis = Visibility::Visible;

        for (body, marker) in missile_q.iter() {
            if !marker.active {
                continue;
            }
            let penalty_str = format!("Power penalty: {}", -marker.power_penalty);
            let timeout_str = if body.flight >= 0 {
                format!("  Timeout in {}", body.flight)
            } else {
                "  Shot timed out...".to_string()
            };
            **text = format!("{}{}", penalty_str, timeout_str);
        }
    }
}
