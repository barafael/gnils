use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode};

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

pub fn aiming_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut turn: ResMut<TurnState>,
    mut players: Query<&mut Player, Without<ShipBlendSprite>>,
    menu: Res<MenuOpen>,
) {
    if turn.round_over || turn.firing || menu.open {
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
            if player.angle < 0.0 { player.angle += 360.0; }
            if player.rel_rot < 0.0 { player.rel_rot += 360.0; }
        }
        if keys.pressed(KeyCode::ArrowRight) {
            player.angle += angle_step;
            player.rel_rot += angle_step;
            if player.angle >= 360.0 { player.angle -= 360.0; }
            if player.rel_rot >= 360.0 { player.rel_rot -= 360.0; }
        }

        if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
            turn.firing = true;
        }
    }
}

pub fn round_over_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut turn: ResMut<TurnState>,
    mut players: Query<&mut Player, Without<ShipBlendSprite>>,
    mut missile_q: Query<(&mut MissileMarker, &mut Visibility), Without<Player>>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut next_state: ResMut<NextState<GamePhase>>,
    menu: Res<MenuOpen>,
) {
    if !turn.round_over || menu.open {
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

        if let Some(image) = images.get_mut(&trail_canvas.image_handle) {
            crate::trail::clear_trail(image);
        }

        let mut p1_score = 0;
        let mut p2_score = 0;
        for player in players.iter() {
            if player.id == 1 { p1_score = player.score; }
            else { p2_score = player.score; }
        }

        if p1_score < p2_score {
            turn.current_player = 1;
        } else if p2_score < p1_score {
            turn.current_player = 2;
        }

        turn.round_over = false;
        turn.firing = false;
        turn.show_round = 100.0;

        for mut player in players.iter_mut() {
            player.power = 100.0;
            player.shot = false;
            player.attempts = 0;
            player.explosion_frame = 0;
            player.rel_rot = 0.01;
            if player.id == 1 { player.angle = 90.0; } else { player.angle = 270.0; }
        }

        for (mut marker, mut vis) in missile_q.iter_mut() {
            marker.active = false;
            *vis = Visibility::Hidden;
        }

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
                menu.selected = 0; // always reset to "Resume"
            }
        }
    }
}

/// Handle navigation and activation inside the settings menu.
pub fn menu_nav_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MenuOpen>,
    mut settings: ResMut<GameSettings>,
    mut players: Query<&mut Player, Without<ShipBlendSprite>>,
    mut turn: ResMut<TurnState>,
    mut next_state: ResMut<NextState<GamePhase>>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut window_q: Query<&mut Window>,
) {
    if !menu.open {
        return;
    }

    // Number of selectable items (non-separator rows)
    const N_ITEMS: usize = 10;

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
        // Resume Game
        0 => { menu.open = false; }
        // New Game
        1 => {
            menu.open = false;
            // Clear trail
            if let Some(image) = images.get_mut(&trail_canvas.image_handle) {
                crate::trail::clear_trail(image);
            }
            for mut player in players.iter_mut() {
                player.score = 0;
                player.power = 100.0;
                player.shot = false;
                player.attempts = 0;
                player.explosion_frame = 0;
                player.rel_rot = 0.01;
                if player.id == 1 { player.angle = 90.0; } else { player.angle = 270.0; }
            }
            turn.round = 0;
            turn.round_over = false;
            turn.firing = false;
            turn.game_over = false;
            turn.show_round = 100.0;
            next_state.set(GamePhase::RoundSetup);
        }
        // Bounce
        2 => { settings.bounce = !settings.bounce; }
        // Fixed Power
        3 => { settings.fixed_power = !settings.fixed_power; }
        // Invisible Planets
        4 => { settings.invisible = !settings.invisible; }
        // Particles
        5 => { settings.particles_enabled = !settings.particles_enabled; }
        // Max Planets (cycle 2→3→4→2)
        6 => {
            settings.max_planets = if left {
                if settings.max_planets <= 2 { 4 } else { settings.max_planets - 1 }
            } else {
                if settings.max_planets >= 4 { 2 } else { settings.max_planets + 1 }
            };
        }
        // Max Blackholes (cycle 0→1→2→3→0)
        7 => {
            settings.max_blackholes = if left {
                if settings.max_blackholes == 0 { 3 } else { settings.max_blackholes - 1 }
            } else {
                (settings.max_blackholes + 1) % 4
            };
        }
        // Rounds (cycle 0=∞ → 5 → 10 → 20 → 0)
        8 => {
            let options = [0u32, 5, 10, 20];
            let idx = options.iter().position(|&v| v == settings.max_rounds).unwrap_or(0);
            let new_idx = if left {
                (idx + options.len() - 1) % options.len()
            } else {
                (idx + 1) % options.len()
            };
            settings.max_rounds = options[new_idx];
        }
        // Fullscreen
        9 => {
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
