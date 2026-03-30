use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode};

use gnils_protocol::ClientMsg;
use lightyear::prelude::MessageSender;

use crate::components::*;
use crate::constants::*;
use crate::resources::*;
use crate::systems::network::send_fire_shot;

pub fn aiming_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut turn: ResMut<TurnState>,
    mut players: Query<&mut Player>,
    menu: Res<MenuOpen>,
    net_mode: Res<NetworkMode>,
    mut senders: Query<&mut MessageSender<ClientMsg>>,
) {
    if turn.round_over || turn.firing || menu.open {
        return;
    }

    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);

    let (power_step, angle_step) = if ctrl {
        (1.0, 0.25_f64.to_radians())
    } else if shift {
        (25.0, 5.0_f64.to_radians())
    } else if alt {
        (0.2, 0.05_f64.to_radians())
    } else {
        (10.0, 2.0_f64.to_radians())
    };

    let current = turn.current_player;

    // In network mode, only the active player (this client's ID) can control the ship
    if let Some(pid) = net_mode.player_id() {
        if current != pid {
            return;
        }
    }

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
            player.angle += angle_step;
            player.rel_rot += angle_step;
            if player.angle >= std::f64::consts::TAU {
                player.angle -= std::f64::consts::TAU;
            }
            if player.rel_rot >= std::f64::consts::TAU {
                player.rel_rot -= std::f64::consts::TAU;
            }
        }
        if keys.pressed(KeyCode::ArrowRight) {
            player.angle -= angle_step;
            player.rel_rot -= angle_step;
            if player.angle < 0.0 {
                player.angle += std::f64::consts::TAU;
            }
            if player.rel_rot < 0.0 {
                player.rel_rot += std::f64::consts::TAU;
            }
        }

        if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
            if net_mode.is_network() {
                send_fire_shot(player.angle, player.power, &mut senders);
            }
            turn.firing = true;
        }
    }
}

pub fn round_over_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut turn: ResMut<TurnState>,
    mut players: Query<&mut Player>,
    mut missile_q: Query<(&mut MissileMarker, &mut Visibility), Without<Player>>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut next_state: ResMut<NextState<GamePhase>>,
    menu: Res<MenuOpen>,
    net_mode: Res<NetworkMode>,
) {
    if !turn.round_over || menu.open {
        return;
    }
    // In network mode the server auto-advances; Space/Enter does nothing here
    if net_mode.is_network() {
        return;
    }

    if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
        if turn.game_over {
            for mut player in players.iter_mut() {
                player.score = 0;
            }
            turn.round = 0;
            turn.game_over = false;
        }

        // Pick who goes first (lower score; player 1 on ties)
        let mut p1_score = 0;
        let mut p2_score = 0;
        for player in players.iter() {
            if player.id == 1 {
                p1_score = player.score;
            } else {
                p2_score = player.score;
            }
        }
        turn.current_player = if p1_score <= p2_score { 1 } else { 2 };

        reset_for_new_round(
            &mut turn,
            &mut players,
            &mut missile_q,
            &trail_canvas,
            &mut images,
        );
        next_state.set(GamePhase::RoundSetup);
    }
}

/// Handle Escape key to open/close the settings menu.
pub fn menu_toggle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MenuOpen>,
    phase: Res<State<GamePhase>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        let allowed = matches!(
            phase.get(),
            GamePhase::Aiming | GamePhase::Firing | GamePhase::RoundOver
        );
        if allowed {
            menu.open = !menu.open;
            if menu.open {
                menu.selected = 0;
            }
        }
    }
}

/// Handle navigation and activation inside the settings menu.
pub fn menu_nav_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MenuOpen>,
    mut settings: ResMut<GameSettings>,
    mut players: Query<&mut Player>,
    mut turn: ResMut<TurnState>,
    mut next_state: ResMut<NextState<GamePhase>>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut missile_q: Query<(&mut MissileMarker, &mut Visibility), Without<Player>>,
    mut window_q: Query<&mut Window>,
    mut net_mode: ResMut<NetworkMode>,
) {
    if !menu.open {
        return;
    }

    const N_ITEMS: usize = 11;

    if keys.just_pressed(KeyCode::ArrowDown) {
        menu.selected = (menu.selected + 1) % N_ITEMS;
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        menu.selected = (menu.selected + N_ITEMS - 1) % N_ITEMS;
    }

    let activate = keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::ArrowRight)
        || keys.just_pressed(KeyCode::ArrowLeft);
    let left = keys.just_pressed(KeyCode::ArrowLeft);

    if !activate {
        return;
    }

    match menu.selected {
        0 => {
            menu.open = false;
        }
        1 => {
            menu.open = false;
            for mut player in players.iter_mut() {
                player.score = 0;
            }
            turn.round = 0;
            turn.game_over = false;
            reset_for_new_round(
                &mut turn,
                &mut players,
                &mut missile_q,
                &trail_canvas,
                &mut images,
            );
            next_state.set(GamePhase::RoundSetup);
        }
        2 => {
            menu.open = false;
            *net_mode = NetworkMode::Local;
            next_state.set(GamePhase::MainMenu);
        }
        3 => {
            settings.bounce = !settings.bounce;
        }
        4 => {
            settings.fixed_power = !settings.fixed_power;
        }
        5 => {
            settings.invisible = !settings.invisible;
        }
        6 => {
            settings.particles_enabled = !settings.particles_enabled;
        }
        7 => {
            settings.max_planets = if left {
                if settings.max_planets <= 2 {
                    4
                } else {
                    settings.max_planets - 1
                }
            } else {
                if settings.max_planets >= 4 {
                    2
                } else {
                    settings.max_planets + 1
                }
            };
        }
        8 => {
            settings.max_blackholes = if left {
                if settings.max_blackholes == 0 {
                    3
                } else {
                    settings.max_blackholes - 1
                }
            } else {
                (settings.max_blackholes + 1) % 4
            };
        }
        9 => {
            let options = [3u32, 5, 10, 20, 0];
            let idx = options
                .iter()
                .position(|&v| v == settings.max_rounds)
                .unwrap_or(0);
            let new_idx = if left {
                (idx + options.len() - 1) % options.len()
            } else {
                (idx + 1) % options.len()
            };
            settings.max_rounds = options[new_idx];
        }
        10 => {
            settings.fullscreen = !settings.fullscreen;
            if let Ok(mut window) = window_q.single_mut() {
                window.mode = if settings.fullscreen {
                    WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                } else {
                    WindowMode::Windowed
                };
            }
        }
        _ => {}
    }
}

/// Shared reset for starting a fresh round.
fn reset_for_new_round(
    turn: &mut TurnState,
    players: &mut Query<&mut Player>,
    missile_q: &mut Query<(&mut MissileMarker, &mut Visibility), Without<Player>>,
    trail_canvas: &TrailCanvas,
    images: &mut Assets<Image>,
) {
    if let Some(image) = images.get_mut(&trail_canvas.image_handle) {
        crate::trail::clear_trail(image);
    }

    turn.round_over = false;
    turn.firing = false;
    turn.show_round = 100.0;

    for mut player in players.iter_mut() {
        player.power = 100.0;
        player.shot = false;
        player.attempts = 0;
        player.explosion_frame = 0;
        player.rel_rot = 0.0;
        player.angle = if player.id == 1 {
            0.0
        } else {
            std::f64::consts::PI
        };
    }

    for (mut marker, mut vis) in missile_q.iter_mut() {
        marker.active = false;
        *vis = Visibility::Hidden;
    }
}
