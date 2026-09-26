use bevy::prelude::*;
use bevy::window::{MonitorSelection, WindowMode};

use gnils_net::{GameEvent, NetMsg, NetState};

use crate::components::*;
use crate::constants::*;
use crate::resources::*;
use crate::systems::network::PendingEdits;
use crate::systems::round::RoundReset;

/// Where Up/Down move the cursor in a list of `n` rows, wrapping at both
/// ends. Every keyboard menu in the game navigates this way.
///
/// `None` means neither key was pressed. Callers must then leave their menu
/// resource untouched: writing to it through a `ResMut` marks it changed even
/// when the value is identical, and the menus redraw on that signal.
pub(crate) fn nav(selected: usize, n: usize, keys: &ButtonInput<KeyCode>) -> Option<usize> {
    let step = match (
        keys.just_pressed(KeyCode::ArrowDown),
        keys.just_pressed(KeyCode::ArrowUp),
    ) {
        (true, false) => 1,
        (false, true) => n - 1,
        // Neither, or both in one frame, which cancel out.
        _ => return None,
    };
    Some((selected + step) % n)
}

/// The aiming keys, in the order [`AimRepeat`] stores their timers.
const AIM_KEYS: [KeyCode; 4] = [
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
];

impl KeyRepeatTimer {
    /// Whether the key steps this frame. Matching the original's
    /// `pygame.key.set_repeat(250, 30)`: fires immediately on press, then
    /// after `KEY_REPEAT_DELAY` of holding, repeats every
    /// `KEY_REPEAT_INTERVAL`.
    fn step(&mut self, key: KeyCode, keys: &ButtonInput<KeyCode>, dt: f32) -> bool {
        if keys.just_pressed(key) {
            self.delay = Some(KEY_REPEAT_DELAY);
            return true;
        }
        if !keys.pressed(key) {
            self.delay = None;
            return false;
        }
        match &mut self.delay {
            Some(d) => {
                *d -= dt;
                if *d <= 0.0 {
                    *d = KEY_REPEAT_INTERVAL;
                    return true;
                }
                false
            }
            None => false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn aiming_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut turn: ResMut<TurnState>,
    mut players: Query<&mut Player>,
    menu: Res<MenuOpen>,
    net_mode: Res<NetworkMode>,
    mut pending: ResMut<PendingEdits>,
    mut repeat: ResMut<AimRepeat>,
) {
    if turn.round_over || turn.firing || menu.open {
        *repeat = AimRepeat::default();
        return;
    }

    let current = turn.current_player;

    // In network mode, only the active player (this client's ID) can control
    // the ship.
    if let Some(pid) = net_mode.player_id()
        && current != pid
    {
        return;
    }

    let held = |a, b| keys.any_pressed([a, b]);
    let (power_step, angle_step) = if held(KeyCode::ControlLeft, KeyCode::ControlRight) {
        (1.0, 0.25_f64.to_radians())
    } else if held(KeyCode::ShiftLeft, KeyCode::ShiftRight) {
        (25.0, 5.0_f64.to_radians())
    } else if held(KeyCode::AltLeft, KeyCode::AltRight) {
        (0.2, 0.05_f64.to_radians())
    } else {
        (10.0, 2.0_f64.to_radians())
    };

    let dt = time.delta_secs();
    let AimRepeat(timers) = &mut *repeat;
    let [up, down, left, right]: [bool; 4] =
        std::array::from_fn(|i| timers[i].step(AIM_KEYS[i], &keys, dt));

    for mut player in players.iter_mut() {
        if player.id != current {
            continue;
        }

        if !settings.fixed_power {
            if up {
                player.power = (player.power + power_step).min(MAX_POWER);
            }
            if down {
                player.power = (player.power - power_step).max(0.0);
            }
        }
        // Left turns anti-clockwise, right clockwise; both angles stay in
        // [0, TAU).
        let turn_by = angle_step * (left as i32 - right as i32) as f64;
        if turn_by != 0.0 {
            player.angle = (player.angle + turn_by).rem_euclid(std::f64::consts::TAU);
            player.rel_rot = (player.rel_rot + turn_by).rem_euclid(std::f64::consts::TAU);
        }

        if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
            if net_mode.is_network() {
                pending
                    .outgoing_broadcast
                    .push(NetMsg::Game(GameEvent::ShotFired {
                        player: current,
                        angle: player.angle,
                        power: player.power,
                    }));
            }
            turn.firing = true;
        }
    }
}

pub fn round_over_input(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<GameSettings>,
    menu: Res<MenuOpen>,
    net_mode: Res<NetworkMode>,
    mut next_state: ResMut<NextState<GamePhase>>,
    mut reset: RoundReset,
) {
    // In network mode the round auto-advances; Space/Enter does nothing here.
    if !reset.turn.round_over || menu.open || net_mode.is_network() {
        return;
    }

    if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::Enter) {
        reset.advance_round(&settings);
        next_state.set(GamePhase::RoundSetup);
    }
}

/// Handle Escape key to open/close the settings menu.
pub fn menu_toggle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MenuOpen>,
    phase: Res<State<GamePhase>>,
) {
    let in_play = matches!(
        phase.get(),
        GamePhase::Aiming | GamePhase::Firing | GamePhase::RoundOver
    );
    if keys.just_pressed(KeyCode::Escape) && in_play {
        menu.open = !menu.open;
        if menu.open {
            menu.selected = 0;
        }
    }
}

/// Handle navigation and activation inside the settings menu.
#[allow(clippy::too_many_arguments)]
pub fn menu_nav_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MenuOpen>,
    mut settings: ResMut<GameSettings>,
    mut next_state: ResMut<NextState<GamePhase>>,
    mut window_q: Query<&mut Window>,
    mut net_mode: ResMut<NetworkMode>,
    net: Res<NetState>,
    mut commands: Commands,
    mut reset: RoundReset,
) {
    if !menu.open {
        return;
    }

    if let Some(row) = nav(menu.selected, MENU_ITEMS.len(), &keys) {
        menu.selected = row;
    }

    let left = keys.just_pressed(KeyCode::ArrowLeft);
    let activate = left
        || keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::ArrowRight);
    if !activate {
        return;
    }

    match menu.item() {
        MenuItem::Resume => menu.open = false,
        MenuItem::NewGame => {
            menu.open = false;
            // Restarting a round mid-network would desync the peers; the
            // network game auto-advances on its own.
            if !net_mode.is_network() {
                reset.reset_scores();
                reset.new_round(&settings);
                next_state.set(GamePhase::RoundSetup);
            }
        }
        MenuItem::MainMenu => {
            menu.open = false;
            *net_mode = NetworkMode::Local;
            crate::systems::network::close_socket(&mut commands, &net);
            next_state.set(GamePhase::MainMenu);
        }
        MenuItem::Bounce => settings.bounce = !settings.bounce,
        MenuItem::FixedPower => settings.fixed_power = !settings.fixed_power,
        MenuItem::Invisible => settings.invisible = !settings.invisible,
        MenuItem::Particles => settings.particles_enabled = !settings.particles_enabled,
        // Planets cycle 2..4, blackholes 0..3, rounds through a fixed list.
        MenuItem::MaxPlanets => {
            settings.max_planets = if left {
                if settings.max_planets <= 2 {
                    4
                } else {
                    settings.max_planets - 1
                }
            } else if settings.max_planets >= 4 {
                2
            } else {
                settings.max_planets + 1
            }
        }
        MenuItem::MaxBlackholes => {
            settings.max_blackholes = if left {
                if settings.max_blackholes == 0 {
                    3
                } else {
                    settings.max_blackholes - 1
                }
            } else {
                (settings.max_blackholes + 1) % 4
            }
        }
        MenuItem::Rounds => {
            const OPTIONS: [u32; 5] = [3, 5, 10, 20, 0];
            let idx = OPTIONS
                .iter()
                .position(|&v| v == settings.max_rounds)
                .unwrap_or(0);
            let step = if left { OPTIONS.len() - 1 } else { 1 };
            settings.max_rounds = OPTIONS[(idx + step) % OPTIONS.len()];
        }
        MenuItem::Fullscreen => toggle_fullscreen(&mut settings, &mut window_q),
        MenuItem::Random => settings.random = !settings.random,
    }
}

/// Flip the fullscreen setting and apply it to the primary window. Web
/// ignores window modes — the canvas fills the viewport either way.
pub(crate) fn toggle_fullscreen(settings: &mut GameSettings, windows: &mut Query<&mut Window>) {
    settings.fullscreen = !settings.fullscreen;
    if let Ok(mut window) = windows.single_mut() {
        window.mode = if settings.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_keys(pressed: &[KeyCode]) -> ButtonInput<KeyCode> {
        let mut keys = ButtonInput::default();
        for &k in pressed {
            keys.press(k);
        }
        keys
    }

    /// An idle frame must report no movement at all. Returning the unchanged
    /// row instead would have callers write it back through a `ResMut`, which
    /// marks the resource changed and makes the menus rebuild every frame.
    #[test]
    fn nav_reports_nothing_when_no_arrow_is_pressed() {
        assert_eq!(nav(3, 12, &with_keys(&[])), None);
        assert_eq!(nav(3, 12, &with_keys(&[KeyCode::Enter])), None);
        // Both arrows in one frame cancel out, so nothing moves.
        assert_eq!(
            nav(3, 12, &with_keys(&[KeyCode::ArrowUp, KeyCode::ArrowDown])),
            None
        );
    }

    #[test]
    fn nav_wraps_at_both_ends() {
        let down = with_keys(&[KeyCode::ArrowDown]);
        let up = with_keys(&[KeyCode::ArrowUp]);
        assert_eq!(nav(0, 12, &down), Some(1));
        assert_eq!(nav(11, 12, &down), Some(0));
        assert_eq!(nav(1, 12, &up), Some(0));
        assert_eq!(nav(0, 12, &up), Some(11));
    }
}
