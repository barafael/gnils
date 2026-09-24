/// Lobby / main menu system.
///
/// Handles keyboard-navigated menus for local and networked play, and the
/// shared-room lobby (the chinese-checkers approach): peers greet with their
/// name, claim one of the two player seats, and the host starts the game
/// explicitly.
use bevy::app::AppExit;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use gnils_net::{NetState, NetMsg, RoomId};

use crate::resources::*;
use crate::systems::network::{
    GameStart, PendingEdits, PendingIncoming, close_socket, host_apply_claim,
};

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

        // Connecting: read-only waiting screen
        app.add_systems(OnEnter(GamePhase::Connecting), spawn_lobby_ui);
        app.add_systems(OnExit(GamePhase::Connecting), despawn_lobby_ui);
        app.add_systems(
            Update,
            (update_lobby_display, cancel_connection_input)
                .run_if(in_state(GamePhase::Connecting)),
        );

        // WaitingForOpponent: the interactive room lobby. Escape still leaves.
        app.add_systems(
            OnEnter(GamePhase::WaitingForOpponent),
            (spawn_lobby_ui, enter_room_lobby),
        );
        app.add_systems(OnExit(GamePhase::WaitingForOpponent), despawn_lobby_ui);
        app.add_systems(
            Update,
            (update_lobby_display, lobby_keyboard_input, cancel_connection_input)
                .run_if(in_state(GamePhase::WaitingForOpponent)),
        );
    }
}

/// The room lobby starts with its first row selected, whatever was selected
/// in the menu that led here, and tells the player what a room is for.
fn enter_room_lobby(mut lobby: ResMut<LobbyMenu>, mut status: ResMut<LobbyStatus>) {
    lobby.selected = 0;
    status.0 = "Claim a seat - share the room id with your opponent.".into();
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

#[allow(clippy::too_many_arguments)]
fn update_lobby_display(
    mut commands: Commands,
    lobby: Res<LobbyMenu>,
    join: Res<JoinRoom>,
    draft: Res<NameDraft>,
    settings: Res<GameSettings>,
    phase: Res<State<GamePhase>>,
    time: Res<Time>,
    net: Res<NetState>,
    status: Res<LobbyStatus>,
    room: Option<Res<RoomId>>,
    root_q: Query<Entity, With<LobbyUi>>,
    children_q: Query<Entity, With<LobbyUiChild>>,
) {
    let in_join = *phase.get() == GamePhase::MainMenu && lobby.screen == LobbyScreen::Join;
    let in_name = lobby.screen == LobbyScreen::Name;
    if !lobby.is_changed()
        && !join.is_changed()
        && !draft.is_changed()
        && !settings.is_changed()
        && !phase.is_changed()
        && !net.is_changed()
        && !status.is_changed()
        && !in_join
        && !in_name
    {
        return;
    }

    for e in children_q.iter() {
        commands.entity(e).despawn();
    }

    let Ok(root) = root_q.single() else { return };
    let cursor = if ((time.elapsed_secs() * 2.0) as u32).is_multiple_of(2) {
        "|"
    } else {
        " "
    };

    let room_name = room.as_ref().map(|r| r.0.as_str());
    for line in build_lines(
        &lobby,
        &join,
        &draft,
        &settings,
        phase.get(),
        cursor,
        &net,
        &status.0,
        room_name,
    ) {
        let child = commands
            .spawn((
                LobbyUiChild,
                Text::new(line.text),
                TextFont {
                    font_size: FontSize::Px(line.size),
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

/// The lobby's line builder: one screen's worth of text.
#[allow(clippy::too_many_arguments)]
fn build_lines(
    lobby: &LobbyMenu,
    join: &JoinRoom,
    draft: &NameDraft,
    settings: &GameSettings,
    phase: &GamePhase,
    cursor: &str,
    net: &NetState,
    status: &str,
    room: Option<&str>,
) -> Vec<UiLine> {
    // The name editor owns the screen wherever it was opened from.
    if lobby.screen == LobbyScreen::Name {
        return vec![
            UiLine::title("YOUR NAME"),
            UiLine::gap(),
            UiLine::info(if draft.0.is_empty() {
                format!("(empty){cursor}")
            } else {
                format!("{}{cursor}", draft.0)
            }),
            UiLine::gap(),
            UiLine::dim("Type a name   Enter apply   Esc cancel"),
        ];
    }

    match phase {
        GamePhase::Connecting => {
            return vec![
                UiLine::title("Connecting..."),
                UiLine::dim(format!("Room: {}", join.text)),
                UiLine::dim("Press Escape to cancel"),
            ];
        }
        GamePhase::WaitingForOpponent => {
            return room_lines(net, status, room, lobby.selected);
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
            const OPTS: &[&str] = &["Host", "Join", "Name", "Back"];
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

        // The room lobby renders by phase (above); the Name editor renders
        // ahead of everything. These arms only exist so the match stays
        // exhaustive.
        LobbyScreen::Room | LobbyScreen::Name => Vec::new(),
    }
}

/// One roster row: who sits at player `n`.
fn seat_text(net: &NetState, seat_no: u8) -> String {
    match net.seats.iter().find(|s| s.player == Some(seat_no)) {
        Some(s) if net.is_me(&s.peer) => format!("Player {seat_no}:  {} (you)", s.name),
        Some(s) => format!("Player {seat_no}:  {}", s.name),
        None => format!("Player {seat_no}:  -- free"),
    }
}

/// The shared-room lobby screen: the roster, the host's start, the way out.
fn room_lines(net: &NetState, status: &str, room: Option<&str>, selected: usize) -> Vec<UiLine> {
    let start_row = if net.is_host {
        "Start Game"
    } else {
        "Start Game (host only)"
    };
    let mut v = vec![
        UiLine::title(format!("ROOM  {}", room.unwrap_or("?"))),
        UiLine::gap(),
        UiLine::sel(seat_text(net, 1), selected == 0),
        UiLine::sel(seat_text(net, 2), selected == 1),
        UiLine::sel(start_row, selected == 2),
        UiLine::sel("Leave", selected == 3),
    ];
    v.push(UiLine::gap());
    if !status.is_empty() {
        v.push(UiLine::info(status.to_string()));
    }
    v.push(UiLine::dim("Enter: claim / release   Esc: leave"));
    v
}

// ── Room decisions ────────────────────────────────────────────────────────────

/// What Enter on seat `n` should do, decided from the roster.
#[derive(Debug, PartialEq, Eq)]
enum ClaimDecision {
    /// The seat is free: ask for it.
    Claim,
    /// We hold the seat: release it.
    Release,
    /// Another player holds it.
    Taken(String),
}

fn claim_decision(net: &NetState, seat: u8) -> ClaimDecision {
    match net.seats.iter().find(|s| s.player == Some(seat)) {
        None => ClaimDecision::Claim,
        Some(s) if net.is_me(&s.peer) => ClaimDecision::Release,
        Some(s) => ClaimDecision::Taken(s.name.clone()),
    }
}

/// Why the host cannot start yet, if it cannot.
fn start_refusal(net: &NetState) -> Option<String> {
    let seated = net
        .seats
        .iter()
        .filter(|s| s.player == Some(1) || s.player == Some(2))
        .count();
    (seated < 2).then(|| "Both seats must be claimed before the game can start.".to_string())
}

// ── Keyboard input ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn lobby_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut key_events: MessageReader<KeyboardInput>,
    mut lobby: ResMut<LobbyMenu>,
    mut join: ResMut<JoinRoom>,
    mut draft: ResMut<NameDraft>,
    mut settings: ResMut<GameSettings>,
    mut next: ResMut<NextState<GamePhase>>,
    phase: Res<State<GamePhase>>,
    mut net_mode: ResMut<NetworkMode>,
    mut net: ResMut<NetState>,
    mut status: ResMut<LobbyStatus>,
    mut pending: ResMut<PendingEdits>,
    mut incoming: ResMut<PendingIncoming>,
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables, unused_mut))]
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    let just = |k: KeyCode| keys.just_pressed(k);

    // The name editor owns the keyboard wherever it is open: typing a name
    // must not also trigger whatever Enter does on the screen beneath it.
    if lobby.screen == LobbyScreen::Name {
        if just(KeyCode::Escape) {
            draft.0.clear();
            back_from_name(&mut lobby, phase.get());
            return;
        }
        if just(KeyCode::Enter) {
            apply_name(&mut lobby, &mut draft, &mut net, &mut status, phase.get());
            return;
        }
        for ev in key_events.read() {
            if ev.state != bevy::input::ButtonState::Pressed {
                continue;
            }
            match ev.key_code {
                KeyCode::Backspace => {
                    draft.0.pop();
                }
                _ => {
                    if let bevy::input::keyboard::Key::Character(ref s) = ev.logical_key
                        && draft.0.chars().count() < 20
                    {
                        draft.0.push_str(s.as_str());
                    }
                }
            }
        }
        return;
    }

    // The shared-room lobby.
    if *phase.get() == GamePhase::WaitingForOpponent {
        const N: usize = 4;
        if just(KeyCode::ArrowDown) {
            lobby.selected = (lobby.selected + 1) % N;
        }
        if just(KeyCode::ArrowUp) {
            lobby.selected = (lobby.selected + N - 1) % N;
        }
        if just(KeyCode::Enter) {
            match lobby.selected {
                0 => seat_action(1, &mut net, &mut pending, &mut status),
                1 => seat_action(2, &mut net, &mut pending, &mut status),
                2 => host_start(
                    &mut net,
                    &mut status,
                    &mut pending,
                    &mut incoming,
                    &settings,
                ),
                _ => leave_room(&mut commands, &net, &mut net_mode, &mut next),
            }
        }
        return;
    }

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
            const N: usize = 4;
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
                        // Host: open the socket for the CLI/URL room and wait
                        // in the lobby.
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
                        draft.0 = net.name.clone();
                        lobby.screen = LobbyScreen::Name;
                        lobby.selected = 0;
                    }
                    3 => {
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
                // Empty keeps the traditional dev-room default; anything typed
                // must be a room name that is safe to put in a URL.
                let room = if join.text.is_empty() {
                    Ok(RoomId("dev-room".to_string()))
                } else {
                    RoomId::parse(&join.text)
                };
                match room {
                    Ok(room) => {
                        join.text = room.0.clone();
                        gnils_net::open_socket_for(&mut commands, room.0);
                        next.set(GamePhase::Connecting);
                    }
                    Err(e) => status.0 = e.to_string(),
                }
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
                        if let bevy::input::keyboard::Key::Character(ref s) = ev.logical_key
                            && join.text.len() < 64
                        {
                            join.text.push_str(s.as_str());
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

        // The room lobby is handled by phase (above); the Name editor by its
        // own screen check (also above). These arms only keep the exhaustive.
        LobbyScreen::Room | LobbyScreen::Name => {}
    }
}

/// Where the name editor hands back to.
fn back_from_name(lobby: &mut LobbyMenu, phase: &GamePhase) {
    lobby.screen = if *phase == GamePhase::WaitingForOpponent {
        LobbyScreen::Room
    } else {
        LobbyScreen::NetworkSub
    };
    lobby.selected = 2;
}

/// Commit the name editor: rename this peer and clear the greeting memory, so
/// the re-greet propagates the new name to the roster.
fn apply_name(
    lobby: &mut LobbyMenu,
    draft: &mut NameDraft,
    net: &mut NetState,
    status: &mut LobbyStatus,
    phase: &GamePhase,
) {
    let name: String = draft.0.trim().chars().take(20).collect();
    if name.is_empty() {
        status.0 = "A name cannot be empty.".into();
        return;
    }
    if name != net.name {
        net.name = name.clone();
        net.greeted.clear();
        status.0 = format!("You are now known as {name}.");
    }
    draft.0.clear();
    back_from_name(lobby, phase);
}

/// Enter on a seat row: claim a free seat, release our own, or refuse one
/// that is taken. The host applies its own claim directly; a guest asks the
/// host and waits for the roster.
fn seat_action(
    seat: u8,
    net: &mut NetState,
    pending: &mut PendingEdits,
    status: &mut LobbyStatus,
) {
    let me = net.my_id.map(|id| id.to_string()).unwrap_or_default();
    match claim_decision(net, seat) {
        ClaimDecision::Claim => {
            if net.is_host {
                host_apply_claim(net, pending, status, &me, Some(seat));
            } else {
                pending.outgoing_broadcast.push(NetMsg::Claim(Some(seat)));
                status.0 = format!("Asking the host for Player {seat}...");
            }
        }
        ClaimDecision::Release => {
            if net.is_host {
                host_apply_claim(net, pending, status, &me, None);
            } else {
                pending.outgoing_broadcast.push(NetMsg::Claim(None));
                status.0 = format!("Releasing Player {seat}...");
            }
        }
        ClaimDecision::Taken(name) => {
            status.0 = format!("Player {seat} is taken by {name}.");
        }
    }
}

/// The host's Start: both seats must be claimed, then the final roster, seed
/// and settings go to everyone — and into our own apply path, since the
/// broadcast does not echo back to its sender.
fn host_start(
    net: &mut NetState,
    status: &mut LobbyStatus,
    pending: &mut PendingEdits,
    incoming: &mut PendingIncoming,
    settings: &GameSettings,
) {
    if let Some(why) = start_refusal(net) {
        status.0 = why;
        return;
    }
    info!(players = net.seats.len(), "host starting the game");
    let seed = gnils_net::new_seed();
    let gs = settings.to_protocol();
    pending
        .outgoing_broadcast
        .push(NetMsg::Start {
            seats: net.seats.clone(),
            seed,
            settings: gs.clone(),
        });
    incoming.start = Some(GameStart {
        seats: net.seats.clone(),
        seed,
        settings: gs,
    });
    status.0 = "Starting...".into();
}

/// Leave the room: drop the socket, forget the session, return to the menu.
/// The player's own name survives inside `NetState`.
fn leave_room(
    commands: &mut Commands,
    net: &NetState,
    net_mode: &mut NetworkMode,
    next: &mut NextState<GamePhase>,
) {
    close_socket(commands, net);
    *net_mode = NetworkMode::Local;
    next.set(GamePhase::MainMenu);
}

// ── Connection cancellation ──────────────────────────────────────────────────

/// Escape during Connecting or in the room lobby leaves the room: closes the
/// socket, forgets the session, and returns to the main menu.
fn cancel_connection_input(
    keys: Res<ButtonInput<KeyCode>>,
    net: Res<NetState>,
    mut next: ResMut<NextState<GamePhase>>,
    mut net_mode: ResMut<NetworkMode>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Escape) {
        leave_room(&mut commands, &net, &mut net_mode, &mut next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnils_net::{PeerId, Seat};

    fn net_with_seats(seats: Vec<Seat>) -> NetState {
        let mut net = NetState::with_name(String::new());
        net.seats = seats;
        net
    }

    fn seat(peer: &str, player: Option<u8>) -> Seat {
        Seat {
            peer: peer.to_string(),
            name: peer.to_string(),
            player,
        }
    }

    #[test]
    fn a_free_seat_can_be_claimed() {
        let net = net_with_seats(vec![]);
        assert_eq!(claim_decision(&net, 1), ClaimDecision::Claim);
    }

    #[test]
    fn our_own_seat_releases() {
        let mut net = net_with_seats(vec![seat("me", Some(1))]);
        net.my_id = Some(PeerId(uuid::Uuid::nil()));
        // Make "me" be us.
        net.seats[0].peer = net.my_id.unwrap().to_string();
        assert_eq!(claim_decision(&net, 1), ClaimDecision::Release);
        assert_eq!(claim_decision(&net, 2), ClaimDecision::Claim);
    }

    #[test]
    fn a_taken_seat_names_its_holder() {
        let net = net_with_seats(vec![seat("bob", Some(2))]);
        assert_eq!(
            claim_decision(&net, 2),
            ClaimDecision::Taken("bob".to_string())
        );
    }

    #[test]
    fn the_host_cannot_start_until_both_seats_are_claimed() {
        let mut net = net_with_seats(vec![seat("a", Some(1))]);
        assert!(start_refusal(&net).is_some());
        net.seats.push(seat("b", Some(2)));
        assert_eq!(start_refusal(&net), None);
        // Spectators do not count as seated.
        net.seats.push(seat("c", None));
        assert_eq!(start_refusal(&net), None);
    }

    #[test]
    fn room_lines_show_the_roster_and_status() {
        let mut net = net_with_seats(vec![seat("bob", Some(2))]);
        net.my_id = Some(PeerId(uuid::Uuid::nil()));
        let me = net.my_id.unwrap().to_string();
        net.seats.push(seat(&me, Some(1)));

        let lines: Vec<String> = room_lines(&net, "hello", Some("rm42"), 0)
            .into_iter()
            .map(|l| l.text)
            .collect();
        let joined = lines.join("\n");
        assert!(joined.contains("ROOM  rm42"), "{joined}");
        assert!(joined.contains("(you)"), "{joined}");
        assert!(joined.contains("Player 2:  bob"), "{joined}");
        assert!(joined.contains("hello"), "{joined}");
        assert!(joined.contains("Start Game"), "{joined}");
        assert!(joined.contains("Leave"), "{joined}");
    }

    #[test]
    fn guest_sees_the_start_row_as_host_only() {
        let net = net_with_seats(vec![]);
        let lines: Vec<String> = room_lines(&net, "", None, 2)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert!(lines.join("\n").contains("(host only)"));
    }
}
