/// Lobby / main menu system.
///
/// Handles keyboard-navigated menus for local and networked play.
use bevy::app::AppExit;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;

use crate::resources::*;

// ── Marker components ─────────────────────────────────────────────────────────

#[derive(Component)]
pub struct LobbyUi;

#[derive(Component)]
struct LobbyUiChild;

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        // MainMenu: full navigation
        app.add_systems(OnEnter(GamePhase::MainMenu), spawn_lobby_ui);
        app.add_systems(OnExit(GamePhase::MainMenu), despawn_lobby_ui);
        app.add_systems(
            Update,
            (update_lobby_display, lobby_keyboard_input)
                .chain()
                .run_if(in_state(GamePhase::MainMenu)),
        );

        // Connecting / WaitingForOpponent: read-only waiting screens
        for phase in [GamePhase::Connecting, GamePhase::WaitingForOpponent] {
            app.add_systems(OnEnter(phase), spawn_lobby_ui);
            app.add_systems(OnExit(phase), despawn_lobby_ui);
        }
        app.add_systems(
            Update,
            (update_lobby_display, cancel_connection_input)
                .run_if(in_state(GamePhase::Connecting).or(in_state(GamePhase::WaitingForOpponent))),
        );
    }
}

// ── Spawn / despawn ───────────────────────────────────────────────────────────

fn spawn_lobby_ui(mut commands: Commands) {
    commands.spawn((
        LobbyUi,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: Val::Px(10.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.9)),
        ZIndex(50),
    ));
}

fn despawn_lobby_ui(mut commands: Commands, q: Query<Entity, With<LobbyUi>>) {
    for e in q.iter() {
        commands.entity(e).despawn();
    }
}

// ── Display ───────────────────────────────────────────────────────────────────

fn update_lobby_display(
    mut commands: Commands,
    lobby: Res<LobbyMenu>,
    join: Res<JoinRoom>,
    settings: Res<GameSettings>,
    phase: Res<State<GamePhase>>,
    time: Res<Time>,
    root_q: Query<Entity, With<LobbyUi>>,
    children_q: Query<Entity, With<LobbyUiChild>>,
) {
    let in_join = *phase.get() == GamePhase::MainMenu && lobby.screen == LobbyScreen::Join;
    if !lobby.is_changed()
        && !join.is_changed()
        && !settings.is_changed()
        && !phase.is_changed()
        && !in_join
    {
        return;
    }

    for e in children_q.iter() {
        commands.entity(e).despawn();
    }

    let Ok(root) = root_q.single() else { return };
    let cursor = if ((time.elapsed_secs() * 2.0) as u32) % 2 == 0 {
        "|"
    } else {
        " "
    };

    for line in build_lines(&lobby, &join, &settings, phase.get(), cursor) {
        let child = commands
            .spawn((
                LobbyUiChild,
                Text::new(line.text),
                TextFont {
                    font_size: line.size,
                    ..default()
                },
                TextColor(line.color),
            ))
            .id();
        commands.entity(root).add_child(child);
    }
}

// ── Line builder ──────────────────────────────────────────────────────────────

struct UiLine {
    text: String,
    size: f32,
    color: Color,
}

impl UiLine {
    fn title(s: impl Into<String>) -> Self {
        Self {
            text: s.into(),
            size: 36.0,
            color: Color::WHITE,
        }
    }
    fn sel(s: impl Into<String>, selected: bool) -> Self {
        Self {
            text: s.into(),
            size: 22.0,
            color: if selected {
                Color::srgb(1.0, 0.9, 0.3)
            } else {
                Color::srgb(0.8, 0.8, 0.8)
            },
        }
    }
    fn info(s: impl Into<String>) -> Self {
        Self {
            text: s.into(),
            size: 20.0,
            color: Color::srgb(0.6, 1.0, 0.6),
        }
    }
    fn dim(s: impl Into<String>) -> Self {
        Self {
            text: s.into(),
            size: 16.0,
            color: Color::srgb(0.45, 0.45, 0.45),
        }
    }
    fn gap() -> Self {
        Self::dim("")
    }
}

fn build_lines(
    lobby: &LobbyMenu,
    join: &JoinRoom,
    settings: &GameSettings,
    phase: &GamePhase,
    cursor: &str,
) -> Vec<UiLine> {
    match phase {
        GamePhase::Connecting => {
            return vec![
                UiLine::title("Connecting..."),
                UiLine::dim(format!("Room: {}", join.text)),
                UiLine::dim("Press Escape to cancel"),
            ];
        }
        GamePhase::WaitingForOpponent => {
            return vec![
                UiLine::title("Waiting for opponent..."),
                UiLine::dim(format!("Room: {}", join.text)),
                UiLine::dim("Press Escape to cancel"),
            ];
        }
        _ => {}
    }

    match lobby.screen {
        LobbyScreen::Main => {
            #[cfg(not(target_arch = "wasm32"))]
            const OPTS: &[&str] = &["New Game", "Network", "Settings", "Help", "Quit"];
            #[cfg(target_arch = "wasm32")]
            const OPTS: &[&str] = &["New Game", "Network", "Settings", "Help"];
            let mut v = vec![UiLine::title("SLINGSHOT"), UiLine::gap()];
            for (i, o) in OPTS.iter().enumerate() {
                let t = if i == lobby.selected {
                    format!("> {o}")
                } else {
                    format!("  {o}")
                };
                v.push(UiLine::sel(t, i == lobby.selected));
            }
            v.push(UiLine::gap());
            v.push(UiLine::dim("Arrow keys navigate   Enter select"));
            v
        }

        LobbyScreen::NetworkSub => {
            const OPTS: &[&str] = &["Host", "Join", "Back"];
            let mut v = vec![UiLine::title("NETWORK"), UiLine::gap()];
            for (i, o) in OPTS.iter().enumerate() {
                let t = if i == lobby.selected {
                    format!("> {o}")
                } else {
                    format!("  {o}")
                };
                v.push(UiLine::sel(t, i == lobby.selected));
            }
            v.push(UiLine::gap());
            v.push(UiLine::dim("Arrow keys navigate   Enter select   Escape back"));
            v
        }

        LobbyScreen::Join => {
            let room = if join.text.is_empty() {
                "dev-room"
            } else {
                &join.text
            };
            let room_line = format!(
                "Room:   {}{}",
                room,
                if lobby.selected == 0 { cursor } else { "" }
            );
            vec![
                UiLine::title("JOIN GAME"),
                UiLine::gap(),
                UiLine::info(room_line),
                UiLine::gap(),
                UiLine::dim("Type room name   Enter to join   Escape back"),
            ]
        }

        LobbyScreen::Settings => {
            let on_off = |b: bool| if b { "On" } else { "Off" };
            let rows: &[(&str, String)] = &[
                ("Max planets", settings.max_planets.to_string()),
                ("Blackholes", settings.max_blackholes.to_string()),
                ("Bounce", on_off(settings.bounce).into()),
                ("Invisible", on_off(settings.invisible).into()),
                ("Fixed power", on_off(settings.fixed_power).into()),
                ("Particles", on_off(settings.particles_enabled).into()),
                ("Max rounds", settings.max_rounds.to_string()),
                ("Max flight", settings.max_flight.to_string()),
                ("Fullscreen", on_off(settings.fullscreen).into()),
                ("Back", String::new()),
            ];
            let mut v = vec![UiLine::title("SETTINGS"), UiLine::gap()];
            for (i, (label, val)) in rows.iter().enumerate() {
                let text = if val.is_empty() {
                    if i == lobby.selected {
                        "> Back".into()
                    } else {
                        "  Back".into()
                    }
                } else {
                    let prefix = if i == lobby.selected { ">" } else { " " };
                    format!("{prefix} {label:<14} {val}")
                };
                v.push(UiLine::sel(text, i == lobby.selected));
            }
            v.push(UiLine::gap());
            v.push(UiLine::dim("Arrow keys select / change   Escape back"));
            v
        }

        LobbyScreen::Help => {
            vec![
                UiLine::title("HOW TO PLAY"),
                UiLine::gap(),
                UiLine::sel("Up / Down    -- adjust power", false),
                UiLine::sel("Left / Right -- adjust angle", false),
                UiLine::sel("Space / Enter  -- fire", false),
                UiLine::sel("Hold Shift  -- 5x coarser", false),
                UiLine::sel("Hold Ctrl   -- fine adjust", false),
                UiLine::gap(),
                UiLine::sel("Hit opponent:  +1500 (minus slow penalty)", false),
                UiLine::sel("Self-hit:      -2000", false),
                UiLine::gap(),
                UiLine::dim("Escape to go back"),
            ]
        }
    }
}

// ── Keyboard input ────────────────────────────────────────────────────────────

fn lobby_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut key_events: MessageReader<KeyboardInput>,
    mut lobby: ResMut<LobbyMenu>,
    mut join: ResMut<JoinRoom>,
    mut settings: ResMut<GameSettings>,
    mut next: ResMut<NextState<GamePhase>>,
    mut net_mode: ResMut<NetworkMode>,
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables, unused_mut))]
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    let just = |k: KeyCode| keys.just_pressed(k);

    match lobby.screen {
        LobbyScreen::Main => {
            #[cfg(not(target_arch = "wasm32"))]
            const N: usize = 5;
            #[cfg(target_arch = "wasm32")]
            const N: usize = 4;
            if just(KeyCode::ArrowDown) {
                lobby.selected = (lobby.selected + 1) % N;
            }
            if just(KeyCode::ArrowUp) {
                lobby.selected = (lobby.selected + N - 1) % N;
            }
            if just(KeyCode::Enter) || just(KeyCode::Space) {
                match lobby.selected {
                    0 => {
                        *net_mode = NetworkMode::Local;
                        next.set(GamePhase::Loading);
                    }
                    1 => {
                        lobby.screen = LobbyScreen::NetworkSub;
                        lobby.selected = 0;
                    }
                    2 => {
                        lobby.screen = LobbyScreen::Settings;
                        lobby.selected = 0;
                    }
                    3 => {
                        lobby.screen = LobbyScreen::Help;
                        lobby.selected = 0;
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    4 => {
                        exit.write(AppExit::Success);
                    }
                    _ => {}
                }
            }
        }

        LobbyScreen::NetworkSub => {
            const N: usize = 3;
            if just(KeyCode::ArrowDown) {
                lobby.selected = (lobby.selected + 1) % N;
            }
            if just(KeyCode::ArrowUp) {
                lobby.selected = (lobby.selected + N - 1) % N;
            }
            if just(KeyCode::Escape) {
                lobby.screen = LobbyScreen::Main;
                lobby.selected = 1;
            }
            if just(KeyCode::Enter) || just(KeyCode::Space) {
                match lobby.selected {
                    0 => {
                        // Host: open the socket for the CLI/URL room and wait.
                        let room = gnils_net::room_id();
                        join.text = room.clone();
                        gnils_net::open_socket_for(&mut commands, room);
                        next.set(GamePhase::Connecting);
                    }
                    1 => {
                        lobby.screen = LobbyScreen::Join;
                        lobby.selected = 0;
                    }
                    2 => {
                        lobby.screen = LobbyScreen::Main;
                        lobby.selected = 1;
                    }
                    _ => {}
                }
            }
        }

        LobbyScreen::Join => {
            if just(KeyCode::Escape) {
                lobby.screen = LobbyScreen::NetworkSub;
                lobby.selected = 1;
                return;
            }
            if just(KeyCode::Enter) {
                let room = if join.text.is_empty() {
                    "dev-room".to_string()
                } else {
                    join.text.clone()
                };
                join.text = room.clone();
                gnils_net::open_socket_for(&mut commands, room);
                next.set(GamePhase::Connecting);
                return;
            }
            for ev in key_events.read() {
                if ev.state != bevy::input::ButtonState::Pressed {
                    continue;
                }
                match ev.key_code {
                    KeyCode::Backspace => {
                        join.text.pop();
                    }
                    _ => {
                        if let bevy::input::keyboard::Key::Character(ref s) = ev.logical_key {
                            if join.text.len() < 64 {
                                join.text.push_str(s.as_str());
                            }
                        }
                    }
                }
            }
        }

        LobbyScreen::Settings => {
            const N: usize = 10;
            if just(KeyCode::ArrowDown) {
                lobby.selected = (lobby.selected + 1) % N;
            }
            if just(KeyCode::ArrowUp) {
                lobby.selected = (lobby.selected + N - 1) % N;
            }
            if just(KeyCode::Escape) || (just(KeyCode::Enter) && lobby.selected == 9) {
                lobby.screen = LobbyScreen::Main;
                lobby.selected = 2;
            }
            let d: i32 = if just(KeyCode::ArrowRight) {
                1
            } else if just(KeyCode::ArrowLeft) {
                -1
            } else {
                0
            };
            if d != 0 {
                match lobby.selected {
                    0 => settings.max_planets = (settings.max_planets as i32 + d).max(1) as u32,
                    1 => {
                        settings.max_blackholes =
                            (settings.max_blackholes as i32 + d).max(0) as u32
                    }
                    2 => settings.bounce = !settings.bounce,
                    3 => settings.invisible = !settings.invisible,
                    4 => settings.fixed_power = !settings.fixed_power,
                    5 => settings.particles_enabled = !settings.particles_enabled,
                    6 => settings.max_rounds = (settings.max_rounds as i32 + d).max(0) as u32,
                    7 => settings.max_flight = (settings.max_flight + d * 50).max(100),
                    8 => settings.fullscreen = !settings.fullscreen,
                    _ => {}
                }
            }
        }

        LobbyScreen::Help => {
            if just(KeyCode::Escape) || just(KeyCode::Enter) || just(KeyCode::Space) {
                lobby.screen = LobbyScreen::Main;
                lobby.selected = 3;
            }
        }
    }
}

// ── Connection cancellation ──────────────────────────────────────────────────

/// Escape cancels a pending connection, closes the socket, and returns to the
/// main menu.
fn cancel_connection_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<GamePhase>>,
    mut net_mode: ResMut<NetworkMode>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Escape) {
        crate::systems::network::close_socket(&mut commands);
        *net_mode = NetworkMode::Local;
        next.set(GamePhase::MainMenu);
    }
}
