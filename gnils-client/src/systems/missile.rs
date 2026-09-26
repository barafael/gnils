use bevy::prelude::*;

use gnils_protocol::compute_launch_velocity;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;
use crate::systems::player::launch_point;
use crate::trail;

/// Launch the missile from the current player's gun, once `turn.firing` has
/// been set — by the local keyboard, or by the sequenced `ShotFired` echo.
pub fn fire_missile(
    mut missile_q: Query<(&mut GravityBody, &mut MissileMarker, &mut Visibility)>,
    mut players: Query<(&mut Player, &Transform)>,
    mut turn: ResMut<TurnState>,
    settings: Res<GameSettings>,
) {
    // Nothing to do unless a shot is pending and the missile is still stowed.
    if !turn.firing || turn.round_over || missile_q.iter().any(|(_, m, _)| m.active) {
        return;
    }

    let current = turn.current_player;
    let Some((mut player, transform)) = players.iter_mut().find(|(p, _)| p.id == current) else {
        return; // no player active
    };

    let launch_pos = launch_point(&player, transform);
    let speed = player.power;
    let velocity = compute_launch_velocity(speed, player.angle);
    let trail_color = player.color_rgb;
    let power_penalty = -(PENALTY_FACTOR * speed) as i32;
    player.attempts += 1;

    for (mut body, mut marker, mut vis) in missile_q.iter_mut() {
        body.pos = launch_pos;
        body.last_pos = launch_pos;
        body.velocity = velocity;
        body.flight = settings.max_flight;

        marker.active = true;
        marker.trail_color = trail_color;
        marker.power_penalty = power_penalty;

        *vis = Visibility::Visible;
    }

    info!(
        "Player {} fires: pos=({:.1},{:.1}) vel=({:.2},{:.2}) power={:.1}",
        current, launch_pos.0, launch_pos.1, velocity.0, velocity.1, speed
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
    let Some(image) = images.get_mut(&trail_canvas.image_handle) else {
        return;
    };
    let image = image.into_inner();

    for (body, marker) in missile_q.iter() {
        if marker.active {
            let (x0, y0) = trail_canvas.to_pixel(body.last_pos);
            let (x1, y1) = trail_canvas.to_pixel(body.pos);
            trail::draw_aa_line(image, x0, y0, x1, y1, marker.trail_color);
        }
    }
}

/// Show the missile only while it is in flight and inside the camera's view.
pub fn update_missile_visibility(
    mut missile_q: Query<(&GravityBody, &MissileMarker, &mut Visibility)>,
    turn: Res<TurnState>,
    proj_q: Query<&Projection, With<Camera2d>>,
) {
    let Ok(Projection::Orthographic(proj)) = proj_q.single() else {
        return;
    };

    for (body, marker, mut vis) in missile_q.iter_mut() {
        let on_screen = proj
            .area
            .contains(Vec2::new(body.pos.0 as f32, body.pos.1 as f32));
        *vis = visibility(turn.firing && marker.active && on_screen);
    }
}

/// Update missile status UI text.
pub fn update_missile_ui(
    missile_q: Query<(&GravityBody, &MissileMarker)>,
    turn: Res<TurnState>,
    mut status_q: Query<(&mut Text, &mut Visibility), With<UiMissileStatus>>,
) {
    for (mut text, mut vis) in status_q.iter_mut() {
        if !turn.firing {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Visible;

        for (body, marker) in missile_q.iter().filter(|(_, m)| m.active) {
            **text = if body.flight >= 0 {
                format!(
                    "Power penalty: {}  Timeout in {}",
                    -marker.power_penalty, body.flight
                )
            } else {
                format!("Power penalty: {}  Shot timed out...", -marker.power_penalty)
            };
        }
    }
}
