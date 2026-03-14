use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use bevy::prelude::*;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use lightyear::webtransport::prelude::{Identity, server::WebTransportServerIo};
use rand::Rng;

use gnils_protocol::*;

// ── Custom channels ─────────────────────────────────────────────────────────

struct Reliable;

struct Unreliable;

// ── Server state ─────────────────────────────────────────────────────────────

#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
enum ServerPhase {
    #[default]
    WaitingForPlayers,
    WaitingForReady,
    RoundSetup,
    WaitingForShot,
    SimulatingFlight,
    RoundOver,
    GameOver,
}

#[derive(Resource, Default)]
struct ConnectedClients {
    clients: Vec<(Entity, PeerId, u8)>,
    ready_count: u8,
}

impl ConnectedClients {
    fn len(&self) -> usize { self.clients.len() }

    fn peer_for_player(&self, player_id: u8) -> Option<PeerId> {
        self.clients.iter().find(|(_, _, pid)| *pid == player_id).map(|(_, p, _)| *p)
    }

    #[allow(dead_code)]
    fn other_peer(&self, peer: PeerId) -> Option<PeerId> {
        self.clients.iter().find(|(_, p, _)| *p != peer).map(|(_, p, _)| *p)
    }
}

#[derive(Resource)]
struct HostSettings(GameSettingsData);
impl Default for HostSettings { fn default() -> Self { Self(GameSettingsData::default()) } }

#[derive(Resource, Default)]
struct RoundState {
    round: u32,
    active_player: u8,
    planets: Vec<PlanetData>,
    player_y: [f64; 2],
    scores: [i32; 2],
    missile: BodySnapshot,
    trail_color: (u8, u8, u8),
    active_peer: Option<PeerId>,
    round_over_timer: f32,
    /// Number of shots fired by each player this round (index 0 = player 1).
    /// Used to compute the quick-hit bonus: hitting the opponent on their 1st/2nd/3rd
    /// attempt awards 500/200/100 bonus points (matching the client).
    player_attempts: [u32; 2],
}

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct GnilsServerPlugin;

impl Plugin for GnilsServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ServerPlugins {
            tick_duration: Duration::from_secs_f64(1.0 / TICK_HZ),
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

        app.init_state::<ServerPhase>();
        app.init_resource::<ConnectedClients>();
        app.init_resource::<HostSettings>();
        app.init_resource::<RoundState>();

        app.add_systems(Startup, start_server);
        app.add_systems(FixedUpdate, (
            receive_client_msgs,
            tick_round_setup.run_if(in_state(ServerPhase::RoundSetup)),
            tick_simulation.run_if(in_state(ServerPhase::SimulatingFlight)),
            tick_round_over.run_if(in_state(ServerPhase::RoundOver)),
        ).chain());

        app.add_observer(on_client_connected);
        app.add_observer(on_client_disconnected);
    }
}

// ── Startup ───────────────────────────────────────────────────────────────────

fn start_server(mut commands: Commands) {
    let port = std::env::var("GNILS_PORT")
        .ok().and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(SERVER_PORT);
    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port);

    let cert = Identity::self_signed(["localhost", "127.0.0.1", "::1"])
        .expect("failed to generate certificate");
    let hash_hex: String = cert.certificate_chain().as_slice()[0].hash().as_ref()
        .iter().map(|b| format!("{b:02x}")).collect();

    println!("CERT_HASH:{hash_hex}");
    info!("gnils-server on {addr}");

    let entity = commands.spawn((
        NetcodeServer::new(NetcodeConfig { protocol_id: PROTOCOL_ID, private_key: PRIVATE_KEY, ..default() }),
        LocalAddr(addr),
        WebTransportServerIo { certificate: cert },
    )).id();
    commands.trigger(Start { entity });
}

// ── Connection observers ──────────────────────────────────────────────────────

fn on_client_connected(
    trigger: On<Add, Connected>,
    remote_ids: Query<&RemoteId>,
    mut clients: ResMut<ConnectedClients>,
    mut next: ResMut<NextState<ServerPhase>>,
) {
    let Ok(remote_id) = remote_ids.get(trigger.entity) else { return };
    let peer = remote_id.0;
    if clients.len() >= 2 { info!("Extra connection from {peer:?} ignored"); return; }

    let player_id = clients.len() as u8 + 1;
    info!("Client {peer:?} -> Player {player_id}");
    clients.clients.push((trigger.entity, peer, player_id));

    if clients.len() == 2 {
        next.set(ServerPhase::WaitingForReady);
    }
}

fn on_client_disconnected(
    trigger: On<Remove, Connected>,
    remote_ids: Query<&RemoteId>,
    mut senders: Query<(&RemoteId, &mut MessageSender<ServerMsg>)>,
    phase: Res<State<ServerPhase>>,
) {
    if !matches!(phase.get(), ServerPhase::WaitingForShot | ServerPhase::SimulatingFlight | ServerPhase::RoundOver) {
        return;
    }
    let gone_peer = remote_ids.get(trigger.entity).ok().map(|r| r.0);
    for (remote_id, mut s) in senders.iter_mut() {
        if Some(remote_id.0) != gone_peer {
            s.send::<Reliable>(ServerMsg::OpponentDisconnected);
            break;
        }
    }
}

// ── Message handling ──────────────────────────────────────────────────────────

fn receive_client_msgs(
    mut receivers: Query<(&RemoteId, &mut MessageReceiver<ClientMsg>)>,
    mut clients: ResMut<ConnectedClients>,
    mut state: ResMut<RoundState>,
    mut next: ResMut<NextState<ServerPhase>>,
    host_settings: Res<HostSettings>,
    phase: Res<State<ServerPhase>>,
    mut senders: Query<(&RemoteId, &mut MessageSender<ServerMsg>)>,
) {
    // Collect messages first to avoid borrow conflicts
    let mut incoming: Vec<(PeerId, ClientMsg)> = Vec::new();
    for (remote_id, mut recv) in receivers.iter_mut() {
        for msg in recv.receive() {
            incoming.push((remote_id.0, msg));
        }
    }

    for (peer, msg) in incoming {
        match msg {
            ClientMsg::Ready => {
                if *phase.get() != ServerPhase::WaitingForReady { continue; }
                clients.ready_count += 1;
                if clients.ready_count >= 2 {
                    for (remote_id, mut s) in senders.iter_mut() {
                        let pid = clients.clients.iter()
                            .find(|(_, p, _)| *p == remote_id.0)
                            .map(|(_, _, id)| *id).unwrap_or(1);
                        s.send::<Reliable>(ServerMsg::GameStart {
                            your_player_id: pid,
                            settings: host_settings.0.clone(),
                        });
                    }
                    next.set(ServerPhase::RoundSetup);
                }
            }
            ClientMsg::AimUpdate { angle, power } => {
                if *phase.get() != ServerPhase::WaitingForShot { continue; }
                if Some(peer) != state.active_peer { continue; }
                for (remote_id, mut s) in senders.iter_mut() {
                    if remote_id.0 != peer {
                        s.send::<Unreliable>(ServerMsg::OpponentAim { angle, power });
                        break;
                    }
                }
            }
            ClientMsg::FireShot { angle, power } => {
                if *phase.get() != ServerPhase::WaitingForShot { continue; }
                if Some(peer) != state.active_peer { continue; }
                launch_missile(&mut state, angle, power, &host_settings.0);
                next.set(ServerPhase::SimulatingFlight);
            }
        }
    }
}

// ── Round setup ───────────────────────────────────────────────────────────────

fn tick_round_setup(
    mut state: ResMut<RoundState>,
    clients: Res<ConnectedClients>,
    host_settings: Res<HostSettings>,
    mut senders: Query<(&RemoteId, &mut MessageSender<ServerMsg>)>,
    mut next: ResMut<NextState<ServerPhase>>,
) {
    state.round += 1;
    state.player_attempts = [0, 0];
    let mut rng = rand::thread_rng();
    state.player_y = [rng.gen_range(PLAYER_Y_MIN..=PLAYER_Y_MAX), rng.gen_range(PLAYER_Y_MIN..=PLAYER_Y_MAX)];
    state.active_player = if state.round == 1 { 1 }
        else if state.scores[0] < state.scores[1] { 1 }
        else if state.scores[1] < state.scores[0] { 2 }
        else { 1 };
    state.active_peer = clients.peer_for_player(state.active_player);
    state.planets = generate_planets(&host_settings.0, &mut rng);

    let msg = ServerMsg::RoundSetup {
        round: state.round,
        active_player: state.active_player,
        planets: state.planets.clone(),
        player_y: state.player_y,
    };
    for (_, mut s) in senders.iter_mut() { s.send::<Reliable>(msg.clone()); }
    next.set(ServerPhase::WaitingForShot);
}

// ── Missile ───────────────────────────────────────────────────────────────────

fn launch_missile(state: &mut RoundState, angle: f64, power: f64, settings: &GameSettingsData) {
    let idx = (state.active_player - 1) as usize;
    state.player_attempts[idx] += 1;
    let x = if state.active_player == 1 { PLAYER1_X } else { PLAYER2_X };
    let gun = if state.active_player == 1 { GUN_OFFSET_P1 } else { GUN_OFFSET_P2 };
    let launch_pos = compute_launch_point(x, state.player_y[idx], gun, angle);
    state.missile = BodySnapshot {
        pos: launch_pos,
        vel: compute_launch_velocity(power, angle),
        last_pos: launch_pos,
        flight: settings.max_flight,
        active: true,
    };
    state.trail_color = if state.active_player == 1 { PLAYER1_COLOR } else { PLAYER2_COLOR };
}

fn tick_simulation(
    mut state: ResMut<RoundState>,
    settings: Res<HostSettings>,
    mut senders: Query<(&RemoteId, &mut MessageSender<ServerMsg>)>,
    mut next: ResMut<NextState<ServerPhase>>,
) {
    if !state.missile.active { return; }

    // Clone read-only data before taking the mutable missile borrow
    let planets = state.planets.clone();
    let trail = state.trail_color;
    let player_y = state.player_y;
    let active_player = state.active_player;

    {
        let m = &mut state.missile;
        step_gravity(&mut m.pos, &mut m.vel, &mut m.last_pos, &mut m.flight, &planets);
        if settings.0.bounce { apply_bounce(m); }
    }

    let snap = state.missile.clone();
    for (_, mut s) in senders.iter_mut() {
        s.send::<Unreliable>(ServerMsg::MissileUpdate { snapshot: snap.clone(), trail_color: trail });
    }

    if let Some(col) = check_collisions(&state.missile, &planets, &player_y, active_player, &settings.0) {
        state.missile.active = false;
        let flight = state.missile.flight;
        let player_attempts = state.player_attempts;
        let (hit, delta) = resolve_hit(&col, active_player, &settings.0, flight, &player_attempts);
        apply_scores(&mut state.scores, active_player, &hit, delta);
        let scores = state.scores;
        let round = state.round;
        let game_over = check_game_over(round, &settings.0);

        if settings.0.particles_enabled {
            let pos = (col.pos.0 as f32, col.pos.1 as f32);
            for (_, mut s) in senders.iter_mut() {
                s.send::<Reliable>(ServerMsg::ParticleSpawn { pos, count: 30, size: 10 });
            }
        }
        for (_, mut s) in senders.iter_mut() {
            s.send::<Unreliable>(ServerMsg::MissileUpdate {
                snapshot: BodySnapshot { active: false, ..snap.clone() },
                trail_color: trail,
            });
            s.send::<Reliable>(ServerMsg::RoundResult { hit: hit.clone(), scores, game_over });
        }

        state.round_over_timer = 0.0;
        next.set(if game_over { ServerPhase::GameOver } else { ServerPhase::RoundOver });
    }
}

fn tick_round_over(
    mut state: ResMut<RoundState>,
    time: Res<Time>,
    mut next: ResMut<NextState<ServerPhase>>,
) {
    state.round_over_timer += time.delta_secs();
    if state.round_over_timer >= 5.0 {
        next.set(ServerPhase::RoundSetup);
    }
}

// ── Collision / physics helpers ───────────────────────────────────────────────

struct ColInfo { pos: (f64, f64), kind: ColKind }

#[derive(Clone)]
enum ColKind { Planet, Blackhole, Ship(u8), Miss }

fn check_collisions(m: &BodySnapshot, planets: &[PlanetData], py: &[f64; 2], active_player: u8, s: &GameSettingsData) -> Option<ColInfo> {
    if m.flight < 0 && !is_on_screen(m.pos) { return Some(ColInfo { pos: m.pos, kind: ColKind::Miss }); }
    if !is_in_extended_range(m.pos)         { return Some(ColInfo { pos: m.pos, kind: ColKind::Miss }); }
    for p in planets {
        let d2 = (m.pos.0-p.pos.0).powi(2)+(m.pos.1-p.pos.1).powi(2);
        if p.is_blackhole { if d2 <= p.mass*p.mass { return Some(ColInfo { pos: m.pos, kind: ColKind::Blackhole }); } }
        else if d2 <= p.radius*p.radius {
            let ip = circle_line_intersect(p.pos, p.radius, m.last_pos, m.pos);
            return Some(ColInfo { pos: ip, kind: ColKind::Planet });
        }
    }
    for (i, &y) in py.iter().enumerate() {
        let ship_id = (i + 1) as u8;
        // Grace period: skip the launching player for the first few ticks to avoid
        // self-hit on launch (gun tip is inside the bounding box at t=0).
        if ship_id == active_player && m.flight > s.max_flight - SELF_HIT_GRACE_TICKS { continue; }
        let x = if i==0 { PLAYER1_X } else { PLAYER2_X };
        for step in 0..10 {
            let tx = m.last_pos.0 + step as f64 * 0.1 * m.vel.0;
            let ty = m.last_pos.1 + step as f64 * 0.1 * m.vel.1;
            if tx>=x-SHIP_HALF_W&&tx<=x+SHIP_HALF_W&&ty>=y-SHIP_HALF_H&&ty<=y+SHIP_HALF_H {
                return Some(ColInfo { pos: (tx,ty), kind: ColKind::Ship(ship_id) });
            }
        }
    }
    None
}

// apply_bounce is provided by gnils_protocol::apply_bounce (imported via use gnils_protocol::*)

fn resolve_hit(col: &ColInfo, active: u8, s: &GameSettingsData, flight: i32, player_attempts: &[u32; 2]) -> (HitResult, i32) {
    match &col.kind {
        ColKind::Planet | ColKind::Miss => (HitResult::Planet, 0),
        ColKind::Blackhole              => (HitResult::Blackhole, 0),
        ColKind::Ship(hit) => {
            let self_hit = *hit == active;
            let penalty = -(s.max_flight - flight.max(0));
            if self_hit {
                (HitResult::Ship { hit_player: *hit, shooter: active, self_hit: true }, -SELF_HIT)
            } else {
                let victim_attempts = player_attempts[(*hit - 1) as usize];
                let quick_bonus = match victim_attempts {
                    1 => QUICK_SCORE_1,
                    2 => QUICK_SCORE_2,
                    3 => QUICK_SCORE_3,
                    _ => 0,
                };
                (HitResult::Ship { hit_player: *hit, shooter: active, self_hit: false }, HIT_SCORE + penalty + quick_bonus)
            }
        }
    }
}

fn apply_scores(scores: &mut [i32;2], active: u8, hit: &HitResult, delta: i32) {
    if let HitResult::Ship {..} = hit { scores[(active-1) as usize] += delta; }
}

fn check_game_over(round: u32, s: &GameSettingsData) -> bool {
    s.max_rounds > 0 && round >= s.max_rounds
}

// generate_planets is provided by gnils_protocol::generate_planets (imported via use gnils_protocol::*)

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(GnilsServerPlugin)
        .run();
}
