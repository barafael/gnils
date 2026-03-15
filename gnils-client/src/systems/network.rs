/// Client-side networking: Lightyear setup, connection management, message send/receive.
///
/// In network mode the server drives all simulation. The client:
///   - Sends AimUpdate (unreliable) while aiming
///   - Sends FireShot (reliable) when Space/Enter is pressed
///   - Receives RoundSetup → spawns planets, positions players, transitions to Aiming
///   - Receives MissileUpdate → writes GravityBody position directly (no local physics)
///   - Receives ParticleSpawn → pushes to ParticleSpawnQueue
///   - Receives RoundResult → populates RoundResult resource, transitions to RoundOver
///   - Receives OpponentAim → updates the opponent Player's angle/power for the aim line

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::netcode::NetcodeClient;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use lightyear::webtransport::prelude::client::WebTransportClientIo;

use gnils_protocol::*;

use crate::components::*;
use crate::resources::*;
use crate::systems::planet::spawn_planet_entities;

// ── Channels (must match server) ──────────────────────────────────────────────

pub struct Reliable;

pub struct Unreliable;

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct GnilsClientNetPlugin;

impl Plugin for GnilsClientNetPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ClientPlugins {
            tick_duration: Duration::from_secs_f64(1.0 / gnils_protocol::TICK_HZ),
        });

        app.add_channel::<Reliable>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        });
        app.add_channel::<Unreliable>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        });

        app.register_message::<ClientMsg>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ServerMsg>()
            .add_direction(NetworkDirection::ServerToClient);

        app.add_systems(OnEnter(GamePhase::Connecting), start_connection);

        app.add_systems(
            Update,
            receive_server_msgs.run_if(not(in_state(GamePhase::MainMenu))
                .and(not(in_state(GamePhase::Connecting)))),
        );

        app.add_systems(
            Update,
            send_aim_update
                .run_if(in_state(GamePhase::Aiming)),
        );

        app.add_observer(on_connected);
        app.add_observer(on_disconnected);
    }
}

// ── Connection ────────────────────────────────────────────────────────────────

fn start_connection(
    mut commands: Commands,
    join_addr: Res<JoinAddress>,
) {
    let addr_str = if join_addr.text.is_empty() {
        "127.0.0.1:5888".to_string()
    } else {
        join_addr.text.clone()
    };

    let server_addr = match SocketAddr::from_str(&addr_str) {
        Ok(a) => a,
        Err(e) => {
            warn!("Invalid server address '{}': {e}", addr_str);
            return;
        }
    };

    info!("Connecting to {server_addr}");

    let auth = Authentication::Manual {
        server_addr,
        client_id: rand_client_id(),
        private_key: PRIVATE_KEY,
        protocol_id: PROTOCOL_ID,
    };
    let netcode = NetcodeClient::new(auth, NetcodeConfig::default())
        .expect("failed to build NetcodeClient");
    let entity = commands.spawn((
        netcode,
        WebTransportClientIo { certificate_digest: join_addr.cert_hash.clone() },
    )).id();
    commands.trigger(Connect { entity });
}

fn rand_client_id() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(12345)
    }
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Math::random() * u64::MAX as f64) as u64
    }
}

fn on_connected(
    _trigger: On<Add, Connected>,
    mut next: ResMut<NextState<GamePhase>>,
    mut senders: Query<&mut MessageSender<ClientMsg>>,
) {
    info!("Connected to server — sending Ready");
    next.set(GamePhase::WaitingForOpponent);
    // Send Ready immediately so server knows we're here
    for mut s in senders.iter_mut() {
        s.send::<Reliable>(ClientMsg::Ready);
    }
}

fn on_disconnected(
    _trigger: On<Remove, Connected>,
    phase: Res<State<GamePhase>>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    warn!("Disconnected from server");
    if !matches!(phase.get(), GamePhase::MainMenu) {
        next.set(GamePhase::MainMenu);
    }
}

// ── Receive server messages ───────────────────────────────────────────────────

pub fn receive_server_msgs(
    mut receivers: Query<&mut MessageReceiver<ServerMsg>>,
    mut net_mode: ResMut<NetworkMode>,
    mut settings: ResMut<GameSettings>,
    mut turn: ResMut<TurnState>,
    mut players: Query<(&mut Player, &mut Transform)>,
    mut missile_q: Query<(&mut GravityBody, &mut MissileMarker, &mut Visibility)>,
    mut spawn_queue: ResMut<ParticleSpawnQueue>,
    mut round_result: ResMut<RoundResult>,
    phase: Res<State<GamePhase>>,
    mut next: ResMut<NextState<GamePhase>>,
    existing_planets: Query<Entity, With<Planet>>,
    mut commands: Commands,
    assets: Option<Res<GameAssets>>,
    trail_canvas: Option<Res<TrailCanvas>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut msgs: Vec<ServerMsg> = Vec::new();
    for mut recv in receivers.iter_mut() {
        for msg in recv.receive() {
            msgs.push(msg);
        }
    }

    for msg in msgs {
        match msg {
            ServerMsg::GameStart { your_player_id, settings: s } => {
                info!("GameStart: you are Player {your_player_id}");
                *net_mode = NetworkMode::Network { player_id: your_player_id };
                settings.apply_from_protocol(&s);
                next.set(GamePhase::Loading);
            }

            ServerMsg::RoundSetup { round, active_player, planets, player_y } => {
                info!("RoundSetup: round={round} active=P{active_player}");
                handle_round_setup(
                    round, active_player, &planets, &player_y,
                    &mut turn, &mut players,
                    &existing_planets, &mut commands, &assets,
                    &mut images, &trail_canvas,
                    &mut next,
                );
            }

            ServerMsg::OpponentAim { angle, power } => {
                let NetworkMode::Network { player_id } = *net_mode else { continue };
                let opponent_id = 3 - player_id;
                for (mut player, _) in players.iter_mut() {
                    if player.id == opponent_id {
                        let initial_angle = if player.id == 1 { 0.0 } else { std::f64::consts::PI };
                        player.angle = angle;
                        player.power = power;
                        player.rel_rot = angle - initial_angle;
                    }
                }
            }

            ServerMsg::MissileUpdate { snapshot, trail_color } => {
                for (mut body, mut marker, mut vis) in missile_q.iter_mut() {
                    marker.active = snapshot.active;
                    marker.trail_color = trail_color;
                    if snapshot.active {
                        body.last_pos = body.pos;
                        body.pos = snapshot.pos;
                        body.velocity = snapshot.vel;
                        body.flight = snapshot.flight;
                        *vis = Visibility::Visible;
                        // Transition Aiming → Firing when missile becomes active
                        if *phase.get() == GamePhase::Aiming {
                            turn.firing = true;
                            next.set(GamePhase::Firing);
                        }
                    } else {
                        *vis = Visibility::Hidden;
                        turn.firing = false;
                    }
                }
            }

            ServerMsg::ParticleSpawn { pos, count, size } => {
                spawn_queue.requests.push(ParticleSpawnRequest {
                    pos: Vec2::new(pos.0, pos.1),
                    count,
                    size,
                });
            }

            ServerMsg::ShotMissed { next_player } => {
                info!("ShotMissed: next player = P{next_player}");
                turn.current_player = next_player;
                turn.firing = false;
                // Deactivate missile
                for (mut _body, mut marker, mut vis) in missile_q.iter_mut() {
                    marker.active = false;
                    *vis = Visibility::Hidden;
                }
                next.set(GamePhase::Aiming);
            }

            ServerMsg::RoundResult { hit, scores, game_over } => {
                info!("RoundResult: {hit:?} scores={scores:?} game_over={game_over}");
                apply_round_result(
                    &hit, &scores, game_over,
                    &mut turn, &mut players, &mut round_result,
                );
                // Clear trail canvas
                if let Some(tc) = &trail_canvas {
                    if let Some(img) = images.get_mut(&tc.image_handle) {
                        crate::trail::clear_trail(img);
                    }
                }
                next.set(GamePhase::RoundOver);
            }

            ServerMsg::OpponentDisconnected => {
                warn!("Opponent disconnected");
                // Return to main menu after showing a message
                next.set(GamePhase::MainMenu);
            }
        }
    }
}

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
) {
    // Despawn old planets
    for e in existing_planets.iter() { commands.entity(e).despawn(); }

    // Spawn new planets
    let Some(ga) = assets else { return };
    spawn_planet_entities(commands, ga, &planets);

    // Position players and reset per-round state (matches Player.init() in the original)
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
        player.angle = if player.id == 1 { 0.0 } else { std::f64::consts::PI };
    }

    // Update turn state
    turn.round = round;
    turn.current_player = active_player;
    turn.last_player = active_player;
    turn.round_over = false;
    turn.firing = false;
    turn.show_round = 100.0;

    // Clear trail canvas
    if let Some(tc) = trail_canvas {
        if let Some(img) = images.get_mut(&tc.image_handle) {
            crate::trail::clear_trail(img);
        }
    }

    next.set(GamePhase::Aiming);
}

fn apply_round_result(
    hit: &HitResult,
    scores: &[i32; 2],
    game_over: bool,
    turn: &mut TurnState,
    players: &mut Query<(&mut Player, &mut Transform)>,
    round_result: &mut RoundResult,
) {
    // Capture old shooter score before overwriting so we can compute the delta for display.
    let score_delta = if let HitResult::Ship { shooter, .. } = hit {
        let old = players.iter().find(|(p, _)| p.id == *shooter).map(|(p, _)| p.score).unwrap_or(0);
        scores[(*shooter - 1) as usize] - old
    } else {
        0
    };

    // Apply scores from server
    for (mut player, _) in players.iter_mut() {
        player.score = scores[(player.id - 1) as usize];
    }

    turn.round_over = true;
    turn.firing = false;
    turn.game_over = game_over;

    *round_result = match hit {
        HitResult::Planet | HitResult::Timeout => RoundResult {
            hit_player: 0, shooter: 0, self_hit: false,
            hit_score: 0, quick_bonus: 0, power_penalty: 0, total_score: 0,
            message: "Missed!".to_string(),
        },
        HitResult::Blackhole => RoundResult {
            hit_player: 0, shooter: 0, self_hit: false,
            hit_score: 0, quick_bonus: 0, power_penalty: 0, total_score: 0,
            message: "Absorbed by blackhole".to_string(),
        },
        HitResult::Ship { hit_player, shooter, self_hit } => {
            if *self_hit {
                RoundResult {
                    hit_player: *hit_player, shooter: *shooter, self_hit: true,
                    hit_score: -SELF_HIT, quick_bonus: 0, power_penalty: 0, total_score: -SELF_HIT,
                    message: format!("Player {} hit themselves!", shooter),
                }
            } else {
                RoundResult {
                    hit_player: *hit_player, shooter: *shooter, self_hit: false,
                    hit_score: HIT_SCORE, quick_bonus: 0, power_penalty: 0, total_score: score_delta,
                    message: format!("Player {} hits Player {}!", shooter, hit_player),
                }
            }
        }
    };

    if game_over { turn.show_round = 100.0; }
}

// ── Send aim updates ──────────────────────────────────────────────────────────

fn send_aim_update(
    net_mode: Res<NetworkMode>,
    players: Query<&Player>,
    turn: Res<TurnState>,
    mut senders: Query<&mut MessageSender<ClientMsg>>,
) {
    let NetworkMode::Network { player_id } = *net_mode else { return };
    if turn.current_player != player_id { return; }

    for player in players.iter() {
        if player.id == player_id {
            for mut s in senders.iter_mut() {
                s.send::<Unreliable>(ClientMsg::AimUpdate {
                    angle: player.angle,
                    power: player.power,
                });
            }
            break;
        }
    }
}

/// Called by the input system when the active player fires in network mode.
pub fn send_fire_shot(
    angle: f64,
    power: f64,
    senders: &mut Query<&mut MessageSender<ClientMsg>>,
) {
    for mut s in senders.iter_mut() {
        s.send::<Reliable>(ClientMsg::FireShot { angle, power });
    }
}

// ── Host subprocess (native only) ─────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_server_process() -> Option<std::process::Child> {
    // Try gnils-server next to current exe, then in PATH
    let server_name = if cfg!(windows) { "gnils-server.exe" } else { "gnils-server" };

    let server_path = std::env::current_exe().ok().and_then(|p| {
        let sibling = p.with_file_name(server_name);
        if sibling.exists() { Some(sibling) } else { None }
    }).unwrap_or_else(|| std::path::PathBuf::from(server_name));

    info!("Spawning server: {}", server_path.display());

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

/// Read the cert hash from a running server's stdout (blocking, called from a thread).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_server_cert_hash(stdout: std::process::ChildStdout) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(stdout);
    for line in reader.lines().flatten() {
        if let Some(hash) = line.strip_prefix("CERT_HASH:") {
            return Some(hash.to_string());
        }
    }
    None
}
