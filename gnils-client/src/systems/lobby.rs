/// Lobby / main menu system.
///
/// Handles keyboard-navigated menus for local and networked play, and the
/// shared-room lobby (the chinese-checkers approach): peers greet with their
/// name, claim one of the two player seats, and the host starts the game
/// explicitly.
use bevy::app::AppExit;
use bevy::clipboard::{Clipboard, ClipboardRead};
use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use gnils_net::{NetMsg, NetState, RoomId, RoomIdError};

use crate::resources::*;
use crate::systems::input::nav;
use crate::systems::network::{GameStart, Room, close_socket};
use crate::systems::rendering::{text_cell, two_col_row};

// ── Marker components ─────────────────────────────────────────────────────────

#[derive(Component)]
pub struct LobbyUi;

#[derive(Component)]
struct LobbyUiChild;

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JoinRoom>()
            .init_resource::<LobbyMenu>()
            .init_resource::<LobbyStatus>()
            .init_resource::<NameDraft>();

        // All three lobby phases show the same panel; they differ only in
        // what the keyboard does.
        for phase in [
            GamePhase::MainMenu,
            GamePhase::Connecting,
            GamePhase::WaitingForOpponent,
        ] {
            app.add_systems(OnEnter(phase), spawn_lobby_ui)
                .add_systems(OnExit(phase), despawn_lobby_ui);
        }

        app.add_systems(
            Update,
            (
                update_lobby_display,
                // MainMenu and the room lobby navigate; Connecting is a
                // read-only waiting screen where only Escape does anything.
                lobby_keyboard_input.run_if(
                    in_state(GamePhase::MainMenu).or_else(in_state(GamePhase::WaitingForOpponent)),
                ),
            )
                .chain()
                .run_if(in_lobby),
        );

        // Escape leaves the room, from the waiting screen and the lobby both.
        app.add_systems(
            Update,
            cancel_connection_input.run_if(
                in_state(GamePhase::Connecting).or_else(in_state(GamePhase::WaitingForOpponent)),
            ),
        );

        // The room lobby seats arrivals by itself.
        app.add_systems(OnEnter(GamePhase::WaitingForOpponent), enter_room_lobby)
            .add_systems(
                Update,
                auto_seat.run_if(in_state(GamePhase::WaitingForOpponent)),
            );
    }
}

/// True in any of the three phases the lobby panel is on screen for.
fn in_lobby(phase: Res<State<GamePhase>>) -> bool {
    matches!(
        phase.get(),
        GamePhase::MainMenu | GamePhase::Connecting | GamePhase::WaitingForOpponent
    )
}

/// The room lobby starts with its first row selected, whatever was selected
/// in the menu that led here, and tells the player what a room is for.
fn enter_room_lobby(mut lobby: ResMut<LobbyMenu>, mut status: ResMut<LobbyStatus>) {
    lobby.selected = 0;
    status.0 = "Share the room code with your opponent.".into();
}

/// The first unclaimed player seat, lowest number first.
fn next_free_seat(seats: &[gnils_net::Seat]) -> Option<u8> {
    (1..=2).find(|&n| !seats.iter().any(|s| s.player == Some(n)))
}

/// Whether both player seats are taken.
fn seats_filled(seats: &[gnils_net::Seat]) -> bool {
    next_free_seat(seats).is_none()
}

/// The row of the room lobby that starts the game.
const START_ROW: usize = 3;
/// How many rows the room lobby has: two seats, copy, start, leave.
const ROOM_ROWS: usize = 5;

/// What `auto_seat` remembers between frames.
#[derive(Default)]
struct AutoSeatMemory {
    /// The seat we last asked for, so we ask only once.
    asked: Option<u8>,
    preselected: bool,
    auto_started: bool,
}

/// Seat newly arrived players automatically: the host takes Player 1, the
/// next arrival Player 2. Switching seats stays possible by hand; nobody has
/// to learn that to play. Also points the host's selection at Start once the
/// room is full, so starting is one Enter.
fn auto_seat(
    mut room: Room,
    mut status: ResMut<LobbyStatus>,
    mut lobby: ResMut<LobbyMenu>,
    settings: Res<GameSettings>,
    room_id: Option<Res<RoomId>>,
    mut memory: Local<AutoSeatMemory>,
) {
    if room_id.is_some_and(|r| r.is_changed()) {
        *memory = AutoSeatMemory::default();
    }
    if room.net.my_player().is_some() {
        memory.asked = None;
    } else if let Some(free) = next_free_seat(&room.net.seats)
        && memory.asked != Some(free)
    {
        memory.asked = Some(free);
        if room.net.is_host {
            let me = room.me();
            room.apply_claim(&mut status, &me, Some(free));
            // The host's own seat exists only once it has greeted; if the
            // grant could not apply yet, retry next frame.
            if room.net.my_player().is_none() {
                memory.asked = None;
            }
        } else {
            room.request_claim(Some(free));
        }
    }

    if !seats_filled(&room.net.seats) {
        memory.preselected = false;
        memory.auto_started = false;
        return;
    }

    if room.net.is_host && !memory.preselected {
        lobby.selected = START_ROW;
    }
    memory.preselected = true;
    // Test hook: the host starts as soon as the room is full (automated
    // two-instance testing needs no keyboard).
    if room.net.is_host && !memory.auto_started && std::env::var("GNILS_AUTOSTART").is_ok() {
        memory.auto_started = true;
        host_start(&mut room, &mut status, &settings);
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
            row_gap: Val::Px(4.0),
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
    // Text fields blink a caret, so their screens redraw every frame;
    // everything else only when something it shows has changed.
    let editing = lobby.screen == LobbyScreen::Name
        || (*phase.get() == GamePhase::MainMenu && lobby.screen == LobbyScreen::Join);
    let changed = lobby.is_changed()
        || join.is_changed()
        || draft.is_changed()
        || settings.is_changed()
        || phase.is_changed()
        || net.is_changed()
        || status.is_changed();
    if !editing && !changed {
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

    for line in build_lines(
        &lobby,
        &join,
        &draft,
        &settings,
        phase.get(),
        cursor,
        &net,
        &status.0,
        room.as_ref().map(|r| r.0.as_str()),
    ) {
        let child = match &line.value {
            // Two-column row: fixed-width label column flush right, value
            // flush left. Fixed widths keep every row's gutters aligned.
            Some(value) => commands
                .spawn((LobbyUiChild, two_col_row()))
                .with_children(|row| {
                    row.spawn(text_cell(
                        &line.text,
                        line.label_width,
                        Justify::Right,
                        line.size,
                        line.color,
                    ));
                    row.spawn(text_cell(
                        value,
                        line.value_width,
                        Justify::Left,
                        line.size,
                        line.color,
                    ));
                })
                .id(),
            None => commands
                .spawn((
                    LobbyUiChild,
                    Text::new(line.text.clone()),
                    TextFont {
                        font_size: FontSize::Px(line.size),
                        ..default()
                    },
                    TextColor(line.color),
                ))
                .id(),
        };
        commands.entity(root).add_child(child);
    }
}

// ── Line builder ──────────────────────────────────────────────────────────────

#[derive(Debug)]
struct UiLine {
    text: String,
    /// `Some(value)` renders a two-column row: the label flush right in a
    /// fixed-width column, the value flush left beside it.
    value: Option<String>,
    size: f32,
    color: Color,
    /// Column widths for two-column rows (layout units).
    label_width: f32,
    value_width: f32,
}

fn selected_color(selected: bool) -> Color {
    if selected {
        Color::srgb(1.0, 0.9, 0.3)
    } else {
        Color::srgb(0.8, 0.8, 0.8)
    }
}

impl UiLine {
    /// A single-column line: just text, in one size and colour.
    fn plain(text: impl Into<String>, size: f32, color: Color) -> Self {
        Self {
            text: text.into(),
            value: None,
            size,
            color,
            label_width: 0.0,
            value_width: 0.0,
        }
    }
    fn title(s: impl Into<String>) -> Self {
        Self::plain(s, 36.0, Color::WHITE)
    }
    fn sel(s: impl Into<String>, selected: bool) -> Self {
        Self::plain(s, 22.0, selected_color(selected))
    }
    fn info(s: impl Into<String>) -> Self {
        Self::plain(s, 20.0, Color::srgb(0.6, 1.0, 0.6))
    }
    fn body(s: impl Into<String>) -> Self {
        Self::plain(s, 16.0, Color::srgb(0.9, 0.9, 0.9))
    }
    fn dim(s: impl Into<String>) -> Self {
        Self::plain(s, 16.0, Color::srgb(0.45, 0.45, 0.45))
    }
    fn gap() -> Self {
        Self::dim("")
    }
    fn two_col(label: &str, value: &str, selected: bool) -> Self {
        Self::two_col_w(label, value, 240.0, 110.0, 22.0, selected)
    }
    fn two_col_w(
        label: &str,
        value: &str,
        label_width: f32,
        value_width: f32,
        size: f32,
        selected: bool,
    ) -> Self {
        Self {
            text: label.to_string(),
            value: Some(value.to_string()),
            size,
            color: selected_color(selected),
            label_width,
            value_width,
        }
    }
}

/// A titled list of options with a hint underneath — the shape of the main
/// menu and the network sub-menu.
fn menu_screen(title: &str, options: &[&str], selected: usize, hint: &str) -> Vec<UiLine> {
    let mut v = vec![UiLine::title(title), UiLine::gap()];
    v.extend(options.iter().enumerate().map(|(i, option)| {
        let caret = if i == selected { ">" } else { " " };
        UiLine::sel(format!("{caret} {option}"), i == selected)
    }));
    v.push(UiLine::gap());
    v.push(UiLine::dim(hint));
    v
}

/// The main menu's options. Quitting is meaningless in a browser tab.
#[cfg(not(target_arch = "wasm32"))]
const MAIN_OPTIONS: &[&str] = &["New Game", "Network", "Settings", "Help", "Quit"];
#[cfg(target_arch = "wasm32")]
const MAIN_OPTIONS: &[&str] = &["New Game", "Network", "Settings", "Help"];

const NETWORK_OPTIONS: &[&str] = &["Host", "Join", "Name", "Back"];

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
            UiLine::info(draft.0.render(cursor)),
            UiLine::gap(),
            UiLine::dim("Type a name   Enter apply   Esc cancel"),
        ];
    }

    match phase {
        GamePhase::Connecting => {
            return vec![
                UiLine::title("Connecting..."),
                UiLine::dim(format!("Room: {}", join.0.text())),
                UiLine::dim("Press Escape to cancel"),
            ];
        }
        GamePhase::WaitingForOpponent => return room_lines(net, status, room, lobby.selected),
        _ => {}
    }

    match lobby.screen {
        LobbyScreen::Main => menu_screen(
            "SLINGSHOT",
            MAIN_OPTIONS,
            lobby.selected,
            "Arrow keys navigate   Enter select",
        ),

        LobbyScreen::NetworkSub => menu_screen(
            "NETWORK",
            NETWORK_OPTIONS,
            lobby.selected,
            "Arrow keys navigate   Enter select   Escape back",
        ),

        LobbyScreen::Join => {
            let field = if join.0.is_empty() {
                format!("(type a room code){cursor}")
            } else {
                join.0.render(cursor)
            };
            vec![
                UiLine::title("JOIN GAME"),
                UiLine::gap(),
                UiLine::info(format!("Room:   {field}")),
                UiLine::gap(),
                UiLine::dim("Room code or invite link   Ctrl+V paste"),
                UiLine::dim("Enter to join   Escape back"),
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
                ("Max rounds", rounds_label(settings.max_rounds)),
                ("Max flight", settings.max_flight.to_string()),
                ("Fullscreen", on_off(settings.fullscreen).into()),
                ("Back", String::new()),
            ];
            let mut v = vec![UiLine::title("SETTINGS"), UiLine::gap()];
            for (i, (label, val)) in rows.iter().enumerate() {
                let selected = i == lobby.selected;
                v.push(if val.is_empty() {
                    UiLine::sel(*label, selected)
                } else {
                    UiLine::two_col(label, val, selected)
                });
            }
            v.push(UiLine::gap());
            v.push(UiLine::dim("Arrow keys select / change   Escape back"));
            v
        }

        LobbyScreen::Help => {
            // The help screen's two columns: a key or term, and what it does.
            let row = |label: &str, value: &str| {
                UiLine::two_col_w(label, value, 170.0, 420.0, 14.0, false)
            };
            vec![
                UiLine::title("HOW TO PLAY"),
                UiLine::gap(),
                UiLine::body("Your goal is to shoot the opponent."),
                UiLine::body("You gain points for hitting them, but every"),
                UiLine::body("bit of power you use costs points - so let"),
                UiLine::body("the gravity of the planets carry your missile."),
                UiLine::gap(),
                row("Up / Down", "increase / decrease power"),
                row("Left / Right", "rotate anti-clockwise / clockwise"),
                row("Space / Enter", "fire"),
                row("Hold Shift", "larger adjustments"),
                row("Hold Ctrl", "smaller adjustments"),
                row("Hold Alt", "very small adjustments"),
                UiLine::gap(),
                row("Hit opponent", "+1500 minus power used"),
                row("Quickhit bonus", "+500 / +200 / +100"),
                row("Self-hit", "-2000"),
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

/// How a round limit reads; the original spells an unlimited game out.
pub(crate) fn rounds_label(max_rounds: u32) -> String {
    if max_rounds == 0 {
        "Infinite".into()
    } else {
        max_rounds.to_string()
    }
}

/// One roster row: who sits at player `n`.
fn seat_text(net: &NetState, seat_no: u8) -> String {
    match net.seats.iter().find(|s| s.player == Some(seat_no)) {
        Some(s) if net.is_me(&s.peer) => format!("Player {seat_no}:  {} (you)", s.name),
        Some(s) => format!("Player {seat_no}:  {}", s.name),
        None => format!("Player {seat_no}:  waiting..."),
    }
}

/// The display name of the current host, if the roster knows them.
fn host_name(net: &NetState) -> Option<&str> {
    let host = net.host_id()?;
    net.seats
        .iter()
        .find(|s| s.peer == host.to_string())
        .map(|s| s.name.as_str())
}

/// The shared-room lobby: the roster, the invite, the host's start, the way
/// out. Rows: seat 1, seat 2, copy invite, start, leave.
fn room_lines(net: &NetState, status: &str, room: Option<&str>, selected: usize) -> Vec<UiLine> {
    #[cfg(target_arch = "wasm32")]
    const COPY_ROW: &str = "Copy invite link";
    #[cfg(not(target_arch = "wasm32"))]
    const COPY_ROW: &str = "Copy room code";

    let start_row: String = match (net.is_host, seats_filled(&net.seats)) {
        (true, true) => "Start Game".into(),
        (true, false) => "Start Game   (waiting for opponent...)".into(),
        (false, _) => format!(
            "Waiting for {} to start...",
            host_name(net).unwrap_or("the host")
        ),
    };

    let mut v = vec![
        UiLine::title(format!("ROOM  {}", room.unwrap_or("?"))),
        UiLine::gap(),
        UiLine::sel(seat_text(net, 1), selected == 0),
        UiLine::sel(seat_text(net, 2), selected == 1),
        UiLine::sel(COPY_ROW, selected == 2),
        UiLine::sel(&start_row, selected == START_ROW),
        UiLine::sel("Leave", selected == 4),
        UiLine::gap(),
    ];
    if !status.is_empty() {
        v.push(UiLine::info(status.to_string()));
    }
    v.push(UiLine::dim("Enter: select   Esc: leave"));
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
    (seated < 2).then(|| "Waiting for your opponent to join...".to_string())
}

// ── Keyboard input ────────────────────────────────────────────────────────────

/// Where Enter on the join field gets the room from: a bare code, or a
/// pasted invite link (`...#room=code`, or an older `...?room=code`), whose
/// code is extracted.
fn extract_room(input: &str) -> Result<RoomId, RoomIdError> {
    let s = input.trim();
    let code = match s.find("room=") {
        Some(i) => s[i + "room=".len()..]
            .split(['&', '#'])
            .next()
            .unwrap_or(""),
        None => s.rsplit('/').next().unwrap_or(s),
    };
    RoomId::parse(code)
}

/// Apply one keyboard event to a text field (caret movement, deletion,
/// character insert). Returns whether anything changed.
fn edit_field(field: &mut TextField, ev: &KeyboardInput, ctrl: bool, max_chars: usize) -> bool {
    if ev.state != bevy::input::ButtonState::Pressed {
        return false;
    }
    match ev.key_code {
        KeyCode::Backspace => {
            if ctrl {
                field.backspace_word()
            } else {
                field.backspace()
            }
        }
        KeyCode::Delete => field.delete(),
        KeyCode::ArrowLeft => field.left(),
        KeyCode::ArrowRight => field.right(),
        KeyCode::Home => field.home(),
        KeyCode::End => field.end(),
        _ => {
            if let bevy::input::keyboard::Key::Character(ref s) = ev.logical_key {
                field.insert(s.as_str(), max_chars);
            } else {
                return false;
            }
        }
    }
    true
}

/// Take a finished clipboard read, if one completed this frame. Multi-line
/// pastes collapse to their first line.
fn poll_paste(read: &mut Option<ClipboardRead>) -> Option<String> {
    let result = read.as_mut()?.poll_result()?;
    *read = None;
    result
        .ok()
        .map(|text| text.lines().next().unwrap_or_default().trim().to_string())
}

/// The longest name the editor accepts.
const MAX_NAME_LEN: usize = 20;

/// The lobby's own state, bundled to keep `lobby_keyboard_input` within
/// Bevy's system-parameter limit.
#[derive(SystemParam)]
struct LobbyState<'w> {
    lobby: ResMut<'w, LobbyMenu>,
    join: ResMut<'w, JoinRoom>,
    draft: ResMut<'w, NameDraft>,
    settings: ResMut<'w, GameSettings>,
    status: ResMut<'w, LobbyStatus>,
    net_mode: ResMut<'w, NetworkMode>,
    next: ResMut<'w, NextState<GamePhase>>,
    phase: Res<'w, State<GamePhase>>,
}

#[allow(clippy::too_many_arguments)]
fn lobby_keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut key_events: MessageReader<KeyboardInput>,
    state: LobbyState,
    mut room: Room,
    mut clipboard: ResMut<Clipboard>,
    room_id: Option<Res<RoomId>>,
    mut windows: Query<&mut Window>,
    mut name_paste: Local<Option<ClipboardRead>>,
    mut join_paste: Local<Option<ClipboardRead>>,
    #[cfg_attr(target_arch = "wasm32", allow(unused_variables, unused_mut))]
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
) {
    let just = |k: KeyCode| keys.just_pressed(k);
    let cmd_or_ctrl = || {
        keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
            || keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight])
    };
    let LobbyState {
        mut lobby,
        mut join,
        mut draft,
        mut settings,
        mut status,
        mut net_mode,
        mut next,
        phase,
    } = state;
    let phase = *phase.get();

    // The name editor owns the keyboard wherever it is open: typing a name
    // must not also trigger whatever Enter does on the screen beneath it.
    if lobby.screen == LobbyScreen::Name {
        if just(KeyCode::Escape) {
            draft.0.clear();
            back_from_name(&mut lobby, phase);
        } else if just(KeyCode::Enter) {
            apply_name(&mut lobby, &mut draft, &mut room.net, &mut status, phase);
        } else {
            if cmd_or_ctrl() && just(KeyCode::KeyV) {
                *name_paste = Some(clipboard.fetch_text());
            }
            if let Some(text) = poll_paste(&mut name_paste) {
                draft.0.insert(&text, MAX_NAME_LEN);
            }
            for ev in key_events.read() {
                edit_field(&mut draft.0, ev, cmd_or_ctrl(), MAX_NAME_LEN);
            }
        }
        return;
    }

    // The shared-room lobby.
    if phase == GamePhase::WaitingForOpponent {
        if let Some(row) = nav(lobby.selected, ROOM_ROWS, &keys) {
            lobby.selected = row;
        }
        if just(KeyCode::Enter) {
            match lobby.selected {
                seat @ (0 | 1) => seat_action(seat as u8 + 1, &mut room, &mut status),
                2 => {
                    if let Some(code) = room_id.as_ref().map(|r| r.0.as_str()) {
                        copy_invite(code, &mut clipboard, &mut status);
                    }
                }
                START_ROW => {
                    if room.net.is_host {
                        host_start(&mut room, &mut status, &settings);
                    } else {
                        status.0 = format!(
                            "Only {} can start the game.",
                            host_name(&room.net).unwrap_or("the host")
                        );
                    }
                }
                _ => leave_room(&mut commands, &room.net, &mut net_mode, &mut next),
            }
        }
        return;
    }

    match lobby.screen {
        LobbyScreen::Main => {
            if let Some(row) = nav(lobby.selected, MAIN_OPTIONS.len(), &keys) {
                lobby.selected = row;
            }
            if just(KeyCode::Enter) || just(KeyCode::Space) {
                match lobby.selected {
                    0 => {
                        info!("MENU: New Game selected");
                        *net_mode = NetworkMode::Local;
                        next.set(GamePhase::Loading);
                    }
                    1 => go_to(&mut lobby, LobbyScreen::NetworkSub),
                    2 => go_to(&mut lobby, LobbyScreen::Settings),
                    3 => go_to(&mut lobby, LobbyScreen::Help),
                    #[cfg(not(target_arch = "wasm32"))]
                    4 => {
                        exit.write(AppExit::Success);
                    }
                    _ => {}
                }
            }
        }

        LobbyScreen::NetworkSub => {
            if let Some(row) = nav(lobby.selected, NETWORK_OPTIONS.len(), &keys) {
                lobby.selected = row;
            }
            if just(KeyCode::Escape) {
                back_to_main(&mut lobby, 1);
            }
            if just(KeyCode::Enter) || just(KeyCode::Space) {
                match lobby.selected {
                    0 => {
                        // Host: open the socket for the CLI/URL room and wait
                        // in the lobby.
                        let new_room = gnils_net::room_id();
                        join.0.set(&new_room);
                        gnils_net::open_socket_for(&mut commands, new_room);
                        next.set(GamePhase::Connecting);
                    }
                    1 => go_to(&mut lobby, LobbyScreen::Join),
                    2 => {
                        draft.0.set(&room.net.name);
                        go_to(&mut lobby, LobbyScreen::Name);
                    }
                    3 => back_to_main(&mut lobby, 1),
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
            if cmd_or_ctrl() && just(KeyCode::KeyV) {
                *join_paste = Some(clipboard.fetch_text());
            }
            if let Some(text) = poll_paste(&mut join_paste) {
                join.0.insert(&text, TextField::MAX_LEN);
            }
            if just(KeyCode::Enter) {
                if join.0.is_empty() {
                    status.0 = "Type the room code your opponent shared.".into();
                    return;
                }
                match extract_room(join.0.text()) {
                    Ok(joined) => {
                        join.0.set(joined.0.as_str());
                        gnils_net::open_socket_for(&mut commands, joined.0);
                        next.set(GamePhase::Connecting);
                    }
                    Err(e) => status.0 = e.to_string(),
                }
                return;
            }
            for ev in key_events.read() {
                edit_field(&mut join.0, ev, cmd_or_ctrl(), TextField::MAX_LEN);
            }
        }

        LobbyScreen::Settings => {
            const BACK_ROW: usize = 9;
            if let Some(row) = nav(lobby.selected, BACK_ROW + 1, &keys) {
                lobby.selected = row;
            }
            if just(KeyCode::Escape) || (just(KeyCode::Enter) && lobby.selected == BACK_ROW) {
                back_to_main(&mut lobby, 2);
            }
            let d: i32 = if just(KeyCode::ArrowRight) {
                1
            } else if just(KeyCode::ArrowLeft) {
                -1
            } else {
                0
            };
            if d != 0 {
                adjust_setting(lobby.selected, d, &mut settings, &mut windows);
            }
        }

        LobbyScreen::Help => {
            if just(KeyCode::Escape) || just(KeyCode::Enter) || just(KeyCode::Space) {
                back_to_main(&mut lobby, 3);
            }
        }

        // The room lobby is handled by phase (above); the Name editor by its
        // own screen check (also above). These arms only keep it exhaustive.
        LobbyScreen::Room | LobbyScreen::Name => {}
    }
}

/// Open a sub-screen with its first row selected.
fn go_to(lobby: &mut LobbyMenu, screen: LobbyScreen) {
    lobby.screen = screen;
    lobby.selected = 0;
}

/// Return to the main menu with the row that led here still selected.
fn back_to_main(lobby: &mut LobbyMenu, row: usize) {
    lobby.screen = LobbyScreen::Main;
    lobby.selected = row;
}

/// Step one row of the pre-game settings screen by `d` (+1 right, -1 left).
fn adjust_setting(
    row: usize,
    d: i32,
    settings: &mut GameSettings,
    windows: &mut Query<&mut Window>,
) {
    match row {
        0 => settings.max_planets = (settings.max_planets as i32 + d).max(1) as u32,
        1 => settings.max_blackholes = (settings.max_blackholes as i32 + d).max(0) as u32,
        2 => settings.bounce = !settings.bounce,
        3 => settings.invisible = !settings.invisible,
        4 => settings.fixed_power = !settings.fixed_power,
        5 => settings.particles_enabled = !settings.particles_enabled,
        6 => settings.max_rounds = (settings.max_rounds as i32 + d).max(0) as u32,
        7 => settings.max_flight = (settings.max_flight + d * 50).max(100),
        8 => crate::systems::input::toggle_fullscreen(settings, windows),
        _ => {}
    }
}

/// Where the name editor hands back to.
fn back_from_name(lobby: &mut LobbyMenu, phase: GamePhase) {
    lobby.screen = if phase == GamePhase::WaitingForOpponent {
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
    phase: GamePhase,
) {
    let name: String = draft.0.text().trim().chars().take(MAX_NAME_LEN).collect();
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

/// Put the room on the clipboard: the invite link on the web (paste it into
/// a browser to land in the room), the bare code on native.
fn copy_invite(room: &str, clipboard: &mut Clipboard, status: &mut LobbyStatus) {
    let payload = gnils_net::invite_url(room).unwrap_or_else(|| room.to_string());
    match clipboard.set_text(payload.as_str()) {
        Ok(()) => {
            #[cfg(target_arch = "wasm32")]
            {
                status.0 = "Invite link copied - paste it to your opponent.".into();
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                status.0 = format!("Room code {room} copied.");
            }
        }
        Err(_) => status.0 = "Could not reach the clipboard.".into(),
    }
}

/// Enter on a seat row: claim a free seat, release our own, or refuse one
/// that is taken. The host applies its own claim directly; a guest asks the
/// host and waits for the roster.
fn seat_action(seat: u8, room: &mut Room, status: &mut LobbyStatus) {
    let decision = claim_decision(&room.net, seat);
    let claim = match &decision {
        ClaimDecision::Claim => Some(seat),
        ClaimDecision::Release => None,
        ClaimDecision::Taken(name) => {
            status.0 = format!("Player {seat} is taken by {name}.");
            return;
        }
    };

    if room.net.is_host {
        let me = room.me();
        room.apply_claim(status, &me, claim);
    } else {
        room.request_claim(claim);
        status.0 = match decision {
            ClaimDecision::Claim => format!("Asking the host for Player {seat}..."),
            _ => format!("Releasing Player {seat}..."),
        };
    }
}

/// The host's Start: both seats must be claimed, then the final roster, seed
/// and settings go to everyone — and into our own apply path, since the
/// broadcast does not echo back to its sender.
fn host_start(room: &mut Room, status: &mut LobbyStatus, settings: &GameSettings) {
    if let Some(why) = start_refusal(&room.net) {
        status.0 = why;
        return;
    }
    info!(players = room.net.seats.len(), "host starting the game");
    let seats = room.net.seats.clone();
    let seed = gnils_net::new_seed();
    let gs = settings.shared.clone();
    room.pending.outgoing_broadcast.push(NetMsg::Start {
        seats: seats.clone(),
        seed,
        settings: gs.clone(),
    });
    room.incoming.start = Some(GameStart {
        seats,
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
        net.is_host = true;
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
        assert!(joined.contains("Copy"), "{joined}");
        assert!(joined.contains("Leave"), "{joined}");
    }

    #[test]
    fn a_guest_sees_whom_they_are_waiting_for() {
        let net = net_with_seats(vec![]);
        let lines: Vec<String> = room_lines(&net, "", None, 3)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert!(lines.join("\n").contains("Waiting for the host to start"));
    }

    #[test]
    fn a_seated_guest_sees_the_host_by_name() {
        let mut net = net_with_seats(vec![seat("ada", Some(1)), seat("bob", Some(2))]);
        let host = PeerId(uuid::Uuid::nil());
        net.peers = vec![host];
        net.my_id = Some(PeerId(uuid::Uuid::max()));
        net.refresh_sorted();
        net.seats[0].peer = host.to_string();

        let lines: Vec<String> = room_lines(&net, "", Some("rm"), 0)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert!(lines.join("\n").contains("Waiting for ada to start"),);
    }

    #[test]
    fn the_first_free_seat_is_the_lowest() {
        assert_eq!(next_free_seat(&[]), Some(1));
        let one = vec![seat("a", Some(1))];
        assert_eq!(next_free_seat(&one), Some(2));
        assert!(!seats_filled(&one));
        let both = vec![seat("a", Some(1)), seat("b", Some(2))];
        assert_eq!(next_free_seat(&both), None);
        assert!(seats_filled(&both));
        // A released seat is free again.
        let released = vec![seat("a", None), seat("b", Some(2))];
        assert_eq!(next_free_seat(&released), Some(1));
    }

    #[test]
    fn text_fields_edit_in_the_middle() {
        let mut f = TextField::default();
        f.set("hello");
        f.home();
        f.right();
        f.right();
        f.insert("XY", 20);
        assert_eq!(f.text(), "heXYllo");
        f.backspace();
        assert_eq!(f.text(), "heXllo");
        f.delete();
        assert_eq!(f.text(), "heXlo");
        f.end();
        f.backspace();
        assert_eq!(f.text(), "heXl");

        let mut w = TextField::default();
        w.set("delete word");
        w.home();
        w.right();
        w.right();
        w.backspace_word();
        assert_eq!(w.text(), "lete word");
    }

    #[test]
    fn text_fields_clamp_and_render_the_caret() {
        let mut f = TextField::default();
        f.insert("abcdef", 4);
        assert_eq!(f.text(), "abcd");
        f.home();
        assert_eq!(f.render("|"), "|abcd");
        f.right();
        f.right();
        assert_eq!(f.render("|"), "ab|cd");
    }

    #[test]
    fn settings_rows_pair_labels_with_values() {
        let lobby = LobbyMenu {
            selected: 0,
            screen: LobbyScreen::Settings,
        };
        let lines: Vec<UiLine> = build_lines(
            &lobby,
            &JoinRoom::default(),
            &NameDraft::default(),
            &GameSettings::default(),
            &GamePhase::MainMenu,
            "",
            &NetState::with_name(String::new()),
            "",
            None,
        );

        // Every setting row is a two-column row pairing its label with its
        // value; the alignment itself is done by the layout, not padding.
        let expected = [
            ("Max planets", "4"),
            ("Blackholes", "0"),
            ("Bounce", "Off"),
            ("Invisible", "Off"),
            ("Fixed power", "Off"),
            ("Particles", "On"),
            ("Max rounds", "Infinite"),
            ("Max flight", "750"),
            ("Fullscreen", "On"),
        ];
        let rows: Vec<&UiLine> = lines.iter().filter(|l| l.value.is_some()).collect();
        assert_eq!(rows.len(), expected.len(), "{lines:?}");
        for (row, (label, value)) in rows.iter().zip(expected) {
            assert_eq!(row.text, label);
            assert_eq!(row.value.as_deref(), Some(value));
            assert_eq!(row.color, selected_color(row.text == "Max planets"));
        }
        // Back is a plain selectable row.
        assert!(
            lines.iter().any(|l| l.value.is_none() && l.text == "Back"),
            "{lines:?}"
        );
    }

    #[test]
    fn join_accepts_codes_and_invite_links() {
        assert_eq!(
            extract_room(" x7k2q ").unwrap(),
            RoomId("x7k2q".to_string())
        );
        assert_eq!(
            extract_room("https://game.example/play?room=ab234&x=1").unwrap(),
            RoomId("ab234".to_string())
        );
        assert_eq!(
            extract_room("https://game.example/play#room=ab234&x=1").unwrap(),
            RoomId("ab234".to_string())
        );
        assert_eq!(
            extract_room("https://game.example/play#x=1&room=ab234").unwrap(),
            RoomId("ab234".to_string())
        );
        assert_eq!(
            extract_room("https://game.example/ab234").unwrap(),
            RoomId("ab234".to_string())
        );
        assert!(extract_room("").is_err());
        assert!(extract_room("has space").is_err());
    }
}
