/// P2P networking via bevy_matchbox (WebRTC).
///
/// Both peers run identical local physics. Messages exchanged:
///   - GameStart (reliable): host → guest, game settings
///   - RoundSetup (reliable): host → guest, planet layout + spawn positions
///   - AimUpdate (unreliable): active player → opponent, live aim preview
///   - FireShot (reliable): active player → opponent, committed shot parameters
///
/// Host = player 1 (lower PeerId). Responsible for generating round setup data.
use bevy::prelude::*;
use bevy_matchbox::prelude::*;

use gnils_protocol::*;

use crate::components::*;
use crate::resources::*;
use crate::systems::planet::spawn_planet_entities;

/// Channel indices into the matchbox socket.
const RELIABLE: usize = 0;
const UNRELIABLE: usize = 1;

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct GnilsClientNetPlugin;

impl Plugin for GnilsClientNetPlugin {
    fn build(&self, app: &mut App) {
        // Open the matchbox socket when entering Connecting state
        app.add_systems(OnEnter(GamePhase::Connecting), open_socket);

        // Poll for peer connection during Connecting / WaitingForOpponent
        app.add_systems(
            Update,
            wait_for_peer.run_if(
                (in_state(GamePhase::Connecting).or(in_state(GamePhase::WaitingForOpponent)))
                    .and(resource_exists::<MatchboxSocket>),
            ),
        );

        // Receive peer messages during gameplay states
        app.add_systems(
            Update,
            receive_peer_msgs.run_if(
                resource_exists::<MatchboxSocket>.and(
                    in_state(GamePhase::RoundSetup)
                        .or(in_state(GamePhase::Aiming))
                        .or(in_state(GamePhase::Firing))
                        .or(in_state(GamePhase::RoundOver))
                        .or(in_state(GamePhase::GameOver)),
                ),
            ),
        );

        // Stream aim updates to opponent while aiming
        app.add_systems(
            Update,
            send_aim_update
                .run_if(in_state(GamePhase::Aiming).and(resource_exists::<MatchboxSocket>)),
        );

        // Host: generate + send RoundSetup on entering that phase
        app.add_systems(
            OnEnter(GamePhase::RoundSetup),
            host_round_setup.run_if(is_host),
        );

        // Network mode: auto-advance from RoundOver after 5 seconds (host only)
        app.add_systems(
            Update,
            network_round_over_tick.run_if(in_state(GamePhase::RoundOver).and(is_network_mode)),
        );

        // Clean up socket when returning to main menu
        app.add_systems(OnEnter(GamePhase::MainMenu), cleanup_network);
    }
}

// ── Run conditions ────────────────────────────────────────────────────────────

fn is_host(net_mode: Res<NetworkMode>) -> bool {
    matches!(*net_mode, NetworkMode::Network { player_id: 1 })
}

fn is_network_mode(net_mode: Res<NetworkMode>) -> bool {
    net_mode.is_network()
}

// ── Connection ────────────────────────────────────────────────────────────────

fn open_socket(mut commands: Commands, join_addr: Res<JoinAddress>) {
    let addr = if join_addr.text.is_empty() {
        format!("127.0.0.1:{}", SIGNALING_PORT)
    } else {
        join_addr.text.clone()
    };

    let url = format!("ws://{addr}/gnils?next=2");
    info!("Opening matchbox socket: {url}");

    let socket: MatchboxSocket = WebRtcSocketBuilder::new(&url)
        .add_reliable_channel()
        .add_unreliable_channel()
        .into();

    commands.insert_resource(socket);
}

fn wait_for_peer(
    mut socket: ResMut<MatchboxSocket>,
    mut net_mode: ResMut<NetworkMode>,
    settings: Res<GameSettings>,
    mut next: ResMut<NextState<GamePhase>>,
    phase: Res<State<GamePhase>>,
) {
    // Check for peer connection / disconnection
    for (peer, state) in socket.update_peers() {
        match state {
            PeerState::Connected => {
                let my_id = socket
                    .id()
                    .expect("socket should have an ID after connection");
                let is_host = my_id < peer;
                let player_id = if is_host { 1 } else { 2 };
                info!(
                    "Peer {peer} connected — we are Player {player_id} ({})",
                    if is_host { "host" } else { "guest" }
                );
                *net_mode = NetworkMode::Network { player_id };

                if is_host {
                    let msg = PeerMsg::GameStart {
                        settings: settings.to_protocol(),
                    };
                    let data = bincode::serialize(&msg).expect("serialize GameStart");
                    socket
                        .channel_mut(RELIABLE)
                        .send(data.into_boxed_slice(), peer);
                }

                next.set(GamePhase::Loading);
            }
            PeerState::Disconnected => {
                warn!("Peer disconnected during connection phase");
                *net_mode = NetworkMode::Local;
                next.set(GamePhase::MainMenu);
            }
        }
    }

    // Transition Connecting → WaitingForOpponent once we have a socket ID
    if *phase.get() == GamePhase::Connecting && socket.id().is_some() {
        next.set(GamePhase::WaitingForOpponent);
    }
}

fn cleanup_network(mut commands: Commands) {
    commands.remove_resource::<MatchboxSocket>();
}

// ── Receive peer messages ─────────────────────────────────────────────────────

pub fn receive_peer_msgs(
    mut socket: ResMut<MatchboxSocket>,
    net_mode: Res<NetworkMode>,
    mut settings: ResMut<GameSettings>,
    mut turn: ResMut<TurnState>,
    mut players: Query<(&mut Player, &mut Transform)>,
    _missile_q: Query<(&GravityBody, &MissileMarker, &Visibility)>,
    _spawn_queue: Res<ParticleSpawnQueue>,
    mut next: ResMut<NextState<GamePhase>>,
    existing_planets: Query<Entity, With<Planet>>,
    mut commands: Commands,
    assets: Option<Res<GameAssets>>,
    trail_canvas: Option<Res<TrailCanvas>>,
    mut images: ResMut<Assets<Image>>,
    mut net_mode_mut: ResMut<NetworkMode>,
) {
    // Check for peer disconnects
    for (_, state) in socket.update_peers() {
        if matches!(state, PeerState::Disconnected) {
            warn!("Opponent disconnected");
            *net_mode_mut = NetworkMode::Local;
            next.set(GamePhase::MainMenu);
            return;
        }
    }

    // Reliable messages
    for (_peer, data) in socket.channel_mut(RELIABLE).receive() {
        let Ok(msg) = bincode::deserialize::<PeerMsg>(&data) else {
            warn!("Failed to deserialize reliable PeerMsg");
            continue;
        };

        match msg {
            PeerMsg::GameStart { settings: s } => {
                info!("Received GameStart from host");
                settings.apply_from_protocol(&s);
            }

            PeerMsg::RoundSetup {
                round,
                active_player,
                planets,
                player_y,
            } => {
                info!("Received RoundSetup: round={round} active=P{active_player}");
                handle_round_setup(
                    round,
                    active_player,
                    &planets,
                    &player_y,
                    &mut turn,
                    &mut players,
                    &existing_planets,
                    &mut commands,
                    &assets,
                    &mut images,
                    &trail_canvas,
                    &mut next,
                    &settings,
                );
            }

            PeerMsg::FireShot { angle, power } => {
                info!("Received FireShot: angle={angle:.2} power={power:.1}");
                // Update the active player's aim to the exact fired values
                let current = turn.current_player;
                for (mut player, _) in players.iter_mut() {
                    if player.id == current {
                        player.angle = angle;
                        player.power = power;
                    }
                }
                // Trigger missile launch (fire_missile system picks this up)
                turn.firing = true;
            }

            PeerMsg::AimUpdate { .. } => {
                // Shouldn't arrive on reliable channel, ignore
            }
        }
    }

    // Unreliable messages
    for (_peer, data) in socket.channel_mut(UNRELIABLE).receive() {
        let Ok(msg) = bincode::deserialize::<PeerMsg>(&data) else {
            continue;
        };

        if let PeerMsg::AimUpdate { angle, power } = msg {
            let NetworkMode::Network { player_id } = *net_mode else {
                continue;
            };
            let opponent_id = 3 - player_id;
            for (mut player, _) in players.iter_mut() {
                if player.id == opponent_id {
                    let initial_angle = if player.id == 1 {
                        0.0
                    } else {
                        std::f64::consts::PI
                    };
                    player.angle = angle;
                    player.power = power;
                    player.rel_rot = angle - initial_angle;
                }
            }
        }
    }
}

// ── Host round setup ──────────────────────────────────────────────────────────

fn host_round_setup(
    mut turn: ResMut<TurnState>,
    mut socket: ResMut<MatchboxSocket>,
    settings: Res<GameSettings>,
    mut players: Query<(&mut Player, &mut Transform)>,
    existing_planets: Query<Entity, With<Planet>>,
    mut commands: Commands,
    assets: Option<Res<GameAssets>>,
    trail_canvas: Option<Res<TrailCanvas>>,
    mut images: ResMut<Assets<Image>>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let round = turn.round + 1;

    let player_y = [
        rng.gen_range(PLAYER_Y_MIN..=PLAYER_Y_MAX),
        rng.gen_range(PLAYER_Y_MIN..=PLAYER_Y_MAX),
    ];

    // Lower score goes first; player 1 on first round
    let active_player = if round == 1 {
        1
    } else {
        let mut p1_score = 0;
        let mut p2_score = 0;
        for (player, _) in players.iter() {
            if player.id == 1 {
                p1_score = player.score;
            } else {
                p2_score = player.score;
            }
        }
        if p1_score <= p2_score { 1 } else { 2 }
    };

    let planets = generate_planets(&settings.to_protocol(), &mut rng);

    // Send to guest
    let msg = PeerMsg::RoundSetup {
        round,
        active_player,
        planets: planets.clone(),
        player_y,
    };
    let data = bincode::serialize(&msg).expect("serialize RoundSetup");
    let peers: Vec<_> = socket.connected_peers().collect();
    for peer in peers {
        socket
            .channel_mut(RELIABLE)
            .send(data.clone().into_boxed_slice(), peer);
    }

    // Apply locally (same path the guest takes when receiving the message)
    handle_round_setup(
        round,
        active_player,
        &planets,
        &player_y,
        &mut turn,
        &mut players,
        &existing_planets,
        &mut commands,
        &assets,
        &mut images,
        &trail_canvas,
        &mut next,
        &settings,
    );
}

// ── Send aim updates ──────────────────────────────────────────────────────────

fn send_aim_update(
    net_mode: Res<NetworkMode>,
    players: Query<&Player>,
    turn: Res<TurnState>,
    mut socket: ResMut<MatchboxSocket>,
) {
    let NetworkMode::Network { player_id } = *net_mode else {
        return;
    };
    if turn.current_player != player_id {
        return;
    }

    for player in players.iter() {
        if player.id == player_id {
            let msg = PeerMsg::AimUpdate {
                angle: player.angle,
                power: player.power,
            };
            let Ok(data) = bincode::serialize(&msg) else {
                return;
            };
            let peers: Vec<_> = socket.connected_peers().collect();
            for peer in peers {
                socket
                    .channel_mut(UNRELIABLE)
                    .send(data.clone().into_boxed_slice(), peer);
            }
            break;
        }
    }
}

/// Called by the input system when the active player fires in network mode.
pub fn send_fire_shot(angle: f64, power: f64, socket: &mut MatchboxSocket) {
    let msg = PeerMsg::FireShot { angle, power };
    let Ok(data) = bincode::serialize(&msg) else {
        return;
    };
    let peers: Vec<_> = socket.connected_peers().collect();
    for peer in peers {
        socket
            .channel_mut(RELIABLE)
            .send(data.clone().into_boxed_slice(), peer);
    }
}

// ── Round-over auto-advance ───────────────────────────────────────────────────

fn network_round_over_tick(
    mut turn: ResMut<TurnState>,
    time: Res<Time>,
    net_mode: Res<NetworkMode>,
    mut next: ResMut<NextState<GamePhase>>,
    mut players: Query<&mut Player>,
    mut missile_q: Query<(&mut MissileMarker, &mut Visibility), Without<Player>>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
) {
    // Only the host auto-advances
    if !matches!(*net_mode, NetworkMode::Network { player_id: 1 }) {
        return;
    }

    turn.round_over_timer += time.delta_secs();
    if turn.round_over_timer >= 5.0 {
        if turn.game_over {
            for mut player in players.iter_mut() {
                player.score = 0;
            }
            turn.round = 0;
            turn.game_over = false;
        }

        // Reset state for new round
        if let Some(image) = images.get_mut(&trail_canvas.image_handle) {
            crate::trail::clear_trail(image);
        }
        turn.round_over = false;
        turn.firing = false;
        turn.show_round = 100.0;
        turn.round_over_timer = 0.0;

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

        next.set(GamePhase::RoundSetup);
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn handle_round_setup(
    round: u32,
    active_player: u8,
    planets: &[PlanetData],
    player_y: &[f64; 2],
    turn: &mut TurnState,
    players: &mut Query<(&mut Player, &mut Transform)>,
    existing_planets: &Query<Entity, With<Planet>>,
    commands: &mut Commands,
    assets: &Option<Res<GameAssets>>,
    images: &mut Assets<Image>,
    trail_canvas: &Option<Res<TrailCanvas>>,
    next: &mut NextState<GamePhase>,
    settings: &GameSettings,
) {
    // Despawn old planets
    for e in existing_planets.iter() {
        commands.entity(e).despawn();
    }

    // Spawn new planets
    let Some(ga) = assets else { return };
    spawn_planet_entities(commands, ga, planets);

    // Position players and reset per-round state
    for (mut player, mut transform) in players.iter_mut() {
        let y = player_y[(player.id - 1) as usize];
        let x = if player.id == 1 { PLAYER1_X } else { PLAYER2_X };
        transform.translation.x = x as f32;
        transform.translation.y = y as f32;

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

    // Update turn state
    turn.round = round;
    turn.current_player = active_player;
    turn.last_player = active_player;
    turn.round_over = false;
    turn.firing = false;
    turn.show_round = 100.0;
    turn.round_over_timer = 0.0;
    turn.show_planets = if settings.invisible { 100.0 } else { 0.0 };

    // Clear trail canvas
    if let Some(tc) = trail_canvas {
        if let Some(img) = images.get_mut(&tc.image_handle) {
            crate::trail::clear_trail(img);
        }
    }

    next.set(GamePhase::Aiming);
}

// ── Host subprocess (native only) ─────────────────────────────────────────────

/// Spawn the matchbox signaling server as a subprocess.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_server_process() -> Option<std::process::Child> {
    let server_name = if cfg!(windows) {
        "gnils-server.exe"
    } else {
        "gnils-server"
    };

    let server_path = std::env::current_exe()
        .ok()
        .and_then(|p| {
            let sibling = p.with_file_name(server_name);
            if sibling.exists() {
                Some(sibling)
            } else {
                None
            }
        })
        .unwrap_or_else(|| std::path::PathBuf::from(server_name));

    info!("Spawning signaling server: {}", server_path.display());

    match std::process::Command::new(&server_path)
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => Some(child),
        Err(e) => {
            warn!("Failed to spawn gnils-server: {e}");
            None
        }
    }
}

/// Read the "SIGNALING_READY" line from the server's stdout (blocking).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_server_ready(stdout: std::process::ChildStdout) -> Option<u16> {
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(stdout);
    for line in reader.lines().flatten() {
        if let Some(port_str) = line.strip_prefix("SIGNALING_READY:") {
            return port_str.parse().ok();
        }
    }
    None
}
