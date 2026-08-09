/// Client-side networking: matchbox P2P, ported from the omdurman approach.
///
/// The simulation is *deterministic lockstep*: both peers run the identical
/// local simulation, and only semantic events cross the wire:
///   - `GameEvent::StartGame` (host commits seed + settings + player ids)
///   - `GameEvent::ShotFired` (the active player's shot)
///   - `Ephemeral::AimUpdate` (unreliable aim-line preview for the opponent)
///
/// Events are submitted as `NetMsg::Game` (guest -> host), sequenced by the
/// host into `NetMsg::Sequenced`, and rebroadcast to every peer (including the
/// host itself via a loopback queue). Every peer applies an event only in its
/// `Sequenced` form, so everyone observes one canonical, ordered stream.
///
/// Planet layouts and player Y positions are derived from a shared seed
/// (`NetSeed`), so the peers need not exchange per-round snapshots.
use bevy::prelude::*;
use gnils_net::*;

use crate::components::*;
use crate::resources::*;
use crate::systems::input::{reset_for_game_start, reset_for_new_round};

/// Frame-scoped staging buffer for reliable outbound messages.
///
/// Systems stage messages here instead of touching the socket directly, and
/// `flush_pending` drains it once per frame. On the host, our own
/// `NetMsg::Game` entries are routed through the loopback queue so
/// `handle_socket` sequences them via the *same* arm as guest submissions — a
/// single serialization point.
#[derive(Resource, Default)]
pub struct PendingEdits {
    /// Reliable broadcast to all peers.
    pub outgoing_broadcast: Vec<NetMsg>,
    /// Reliable send to a single peer.
    pub outgoing_targeted: Vec<(NetMsg, PeerId)>,
}

#[derive(Resource, Default)]
pub struct PendingIncoming {
    /// Sequenced game events received from `handle_socket`, applied by
    /// `apply_game_events`.
    pub live: Vec<(GameEvent, PeerId)>,
    /// Ephemeral display messages applied by `apply_ephemeral`.
    pub ephemeral: Vec<(Ephemeral, PeerId)>,
    /// Host-only: events the host just sequenced, queued to feed back through
    /// its own receive path next frame so the host applies its own events on
    /// the same apply-on-echo path as everyone else.
    pub loopback: Vec<NetMsg>,
}

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(NetState::default())
            .insert_resource(PendingEdits::default())
            .insert_resource(PendingIncoming::default())
            .add_systems(
                Update,
                (
                    handle_socket,
                    transition_connecting.after(handle_socket),
                    flush_pending.after(handle_socket),
                    apply_game_events.after(handle_socket),
                    apply_ephemeral.after(handle_socket),
                    broadcast_aim.after(handle_socket),
                    network_auto_advance.after(handle_socket),
                ),
            );
    }
}

/// Reset all net-side state and drop the socket (used when leaving a network
/// session back to the main menu).
pub(crate) fn close_socket(commands: &mut Commands) {
    commands.remove_resource::<MatchboxSocket>();
    commands.insert_resource(NetState::default());
    commands.insert_resource(PendingEdits::default());
    commands.insert_resource(PendingIncoming::default());
}

// ── Socket processing ───────────────────────────────────────────────────────

/// Once the matchbox socket has received its id (i.e. connected to the
/// signaling server), leave `Connecting` and wait for an opponent.
fn transition_connecting(
    net: Res<NetState>,
    state: Res<State<GamePhase>>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    if *state.get() == GamePhase::Connecting && net.my_id.is_some() {
        next.set(GamePhase::WaitingForOpponent);
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_socket(
    socket: Option<ResMut<MatchboxSocket>>,
    mut net: ResMut<NetState>,
    mut pending: ResMut<PendingEdits>,
    mut incoming: ResMut<PendingIncoming>,
    net_mode: Res<NetworkMode>,
    settings: Res<GameSettings>,
    state: Res<State<GamePhase>>,
    mut started: Local<bool>,
) {
    let Some(mut socket) = socket else {
        return;
    };

    let Ok(peer_updates) = socket.try_update_peers() else {
        return;
    };
    let mut peers_changed = false;
    for (peer, peer_state) in peer_updates {
        match peer_state {
            PeerState::Connected if !net.peers.contains(&peer) => {
                net.peers.push(peer);
                peers_changed = true;
                info!(%peer, "peer connected");
            }
            PeerState::Disconnected => {
                let before = net.peers.len();
                net.peers.retain(|&p| p != peer);
                peers_changed |= net.peers.len() != before;
                info!(%peer, "peer disconnected");
            }
            _ => {}
        }
    }

    let my_id_just_set = net.my_id.is_none() && socket.id().is_some();
    if my_id_just_set {
        net.my_id = socket.id();
    }
    if peers_changed || my_id_just_set {
        net.refresh_sorted();
    }

    if let Some(my_id) = net.my_id
        && (peers_changed || my_id_just_set)
    {
        net.is_host = net.sorted_all().first() == Some(&my_id);
        if net.is_host {
            info!("host election: this peer is the host");
        }
    }

    // Host: auto-start the game once both players are present and the game
    // hasn't begun yet. `started` is a frame-local latch so we don't re-send
    // StartGame every frame while the echo is still in flight.
    if net.peers.len() != 1 {
        *started = false;
    }
    if net.is_host
        && !*started
        && net.peers.len() == 1
        && !net_mode.is_network()
        && *state.get() == GamePhase::WaitingForOpponent
    {
        *started = true;
        let assignments: Vec<(PeerId, u8)> = net
            .sorted_all()
            .iter()
            .enumerate()
            .map(|(i, &p)| (p, i as u8 + 1))
            .collect();
        let ev = GameEvent::StartGame {
            seed: new_seed(),
            settings: settings.to_protocol(),
            assignments,
        };
        info!("host: both players present, starting game");
        pending.outgoing_broadcast.push(NetMsg::Game(ev));
    }

    let mut targeted: Vec<(NetMsg, PeerId)> = Vec::new();
    let mut sequenced_out: Vec<NetMsg> = Vec::new();
    let is_host = net.is_host;

    let reliable: Vec<(PeerId, Box<[u8]>)> = socket.channel_mut(CH_RELIABLE).receive();
    let unreliable: Vec<(PeerId, Box<[u8]>)> = socket.channel_mut(CH_UNRELIABLE).receive();

    // Host loopback: events the host sequenced for itself. They flow through
    // the identical apply path as remote `Sequenced` events.
    let my_id = net.my_id.unwrap_or(PeerId(uuid::Uuid::nil()));
    let loopback: Vec<(PeerId, NetMsg)> = incoming
        .loopback
        .drain(..)
        .map(|msg| (my_id, msg))
        .collect();

    let decoded = reliable
        .into_iter()
        .chain(unreliable)
        .filter_map(|(peer, raw)| match decode(&raw) {
            Some(msg) => Some((peer, msg)),
            None => {
                warn!("unknown message, ignoring");
                None
            }
        })
        .chain(loopback);

    for (peer, msg) in decoded {
        match msg {
            NetMsg::Game(ev) => {
                if !is_host {
                    // We received an unsequenced submission but don't believe we
                    // are the host — likely a transient election disagreement
                    // right after a peer connect. Re-forward it to whoever we
                    // currently consider the host.
                    match net.host_id() {
                        Some(host) => {
                            targeted.push((NetMsg::Game(ev), host));
                        }
                        None => {
                            pending.outgoing_broadcast.push(NetMsg::Game(ev));
                        }
                    }
                    continue;
                }
                let seq = net.next_seq;
                net.next_seq += 1;
                let sequenced = NetMsg::Sequenced { seq, event: ev };
                sequenced_out.push(sequenced.clone());
                // Push the echo onto our own loopback queue; it is applied next
                // frame through the same path as every other peer.
                incoming.loopback.push(sequenced);
            }
            NetMsg::Sequenced { seq, event: ev } => {
                // Apply each sequence number exactly once. The reliable channel
                // is ordered and `seq` is monotonic, so anything at or below the
                // highest applied is a duplicate delivery.
                if net.last_applied_seq.is_some_and(|last| seq <= last) {
                    continue;
                }
                net.last_applied_seq = Some(seq);
                incoming.live.push((ev, peer));
            }
            NetMsg::Ephemeral(eph) => {
                incoming.ephemeral.push((eph, peer));
            }
        }
    }

    for (msg, peer) in targeted {
        pending.outgoing_targeted.push((msg, peer));
    }
    for msg in sequenced_out {
        pending.outgoing_broadcast.push(msg);
    }
}

// ── Outbound flushing ───────────────────────────────────────────────────────

fn flush_pending(
    mut pending: ResMut<PendingEdits>,
    mut incoming: ResMut<PendingIncoming>,
    net: Res<NetState>,
    mut socket: Option<ResMut<MatchboxSocket>>,
) {
    if pending.outgoing_broadcast.is_empty() && pending.outgoing_targeted.is_empty() {
        return;
    }

    let i_sequence = net.is_host || net.peers.is_empty();
    let host = net.host_id();

    let staged: Vec<NetMsg> = std::mem::take(&mut pending.outgoing_broadcast);
    let mut to_broadcast: Vec<NetMsg> = Vec::new();
    let mut retained_broadcast: Vec<NetMsg> = Vec::new();

    for msg in staged {
        match msg {
            NetMsg::Game(event) if i_sequence => {
                // The host sequences its own events through the same arm as
                // guest submissions (in `handle_socket`), so it loops them back
                // unsequenced rather than sequencing here.
                incoming.loopback.push(NetMsg::Game(event));
            }
            NetMsg::Game(event) => {
                let submission = NetMsg::Game(event);
                let sent = match (host, enc_msg(&submission), socket.as_deref_mut()) {
                    (Some(host), Some(encoded), Some(socket)) => socket
                        .channel_mut(CH_RELIABLE)
                        .try_send(encoded, host)
                        .inspect_err(|e| warn!(error = %e, "submit to host failed; will retry"))
                        .is_ok(),
                    _ => false,
                };
                if !sent {
                    retained_broadcast.push(submission);
                }
            }
            other => to_broadcast.push(other),
        }
    }

    let targeted: Vec<(NetMsg, PeerId)> = std::mem::take(&mut pending.outgoing_targeted);
    let mut retained_targeted: Vec<(NetMsg, PeerId)> = Vec::new();
    for (msg, peer) in targeted {
        let sent = match (enc_msg(&msg), socket.as_deref_mut()) {
            (Some(encoded), Some(socket)) => socket
                .channel_mut(CH_RELIABLE)
                .try_send(encoded, peer)
                .inspect_err(|e| warn!(error = %e, "reliable targeted send failed; will retry"))
                .is_ok(),
            _ => false,
        };
        if !sent {
            retained_targeted.push((msg, peer));
        }
    }

    for msg in to_broadcast {
        if net.peers.is_empty() {
            retained_broadcast.push(msg);
            continue;
        }
        let Some(socket) = socket.as_deref_mut() else {
            retained_broadcast.push(msg);
            continue;
        };
        let Some(encoded) = enc_msg(&msg) else {
            retained_broadcast.push(msg);
            continue;
        };
        let channel = socket.channel_mut(CH_RELIABLE);
        let mut all_ok = true;
        for &peer in &net.peers {
            if let Err(e) = channel.try_send(encoded.clone(), peer) {
                warn!(error = %e, "reliable broadcast send failed; will retry");
                all_ok = false;
            }
        }
        if !all_ok {
            retained_broadcast.push(msg);
        }
    }

    pending.outgoing_broadcast = retained_broadcast;
    pending.outgoing_targeted = retained_targeted;
}

// ── Event application ───────────────────────────────────────────────────────

fn apply_game_events(
    mut incoming: ResMut<PendingIncoming>,
    mut net_mode: ResMut<NetworkMode>,
    mut settings: ResMut<GameSettings>,
    mut net_seed: ResMut<NetSeed>,
    mut turn: ResMut<TurnState>,
    net: Res<NetState>,
    mut players: Query<&mut Player>,
    mut missile_q: Query<(&mut MissileMarker, &mut Visibility), Without<Player>>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    for (ev, _peer) in incoming.live.drain(..) {
        match ev {
            GameEvent::StartGame {
                seed,
                settings: gs,
                assignments,
            } => {
                if net_mode.is_network() {
                    // Duplicate / late StartGame while already playing.
                    continue;
                }
                let my_id = net.my_id;
                let player_id = my_id
                    .and_then(|id| {
                        assignments
                            .iter()
                            .find(|(p, _)| *p == id)
                            .map(|(_, pid)| *pid)
                    })
                    .or_else(|| my_id.and_then(|id| net.player_id_for(id)))
                    .unwrap_or(1);
                *net_mode = NetworkMode::Network { player_id };
                settings.apply_from_protocol(&gs);
                net_seed.base = seed;
                info!(seed, player_id, "game started via host StartGame");
                reset_for_game_start(
                    &mut turn,
                    &mut players,
                    &mut missile_q,
                    &trail_canvas,
                    &mut images,
                    &settings,
                );
                next.set(GamePhase::Loading);
            }
            GameEvent::ShotFired {
                player,
                angle,
                power,
            } => {
                if !net_mode.is_network() {
                    continue;
                }
                // Write the exact launch parameters onto the shooter's player
                // so the remote peer launches identically (on the shooter the
                // ECS values already match; this is a no-op).
                for mut p in players.iter_mut() {
                    if p.id == player {
                        let initial = if p.id == 1 { 0.0 } else { std::f64::consts::PI };
                        p.angle = angle;
                        p.power = power;
                        p.rel_rot = angle - initial;
                    }
                }
                // `turn.firing` flips on and `fire_missile` (FixedUpdate, in
                // Aiming) launches the missile on the next fixed tick, through
                // the *same* code path as local play. `fire_transition_system`
                // then moves both peers Aiming -> Firing.
                turn.firing = true;
            }
        }
    }
}

fn apply_ephemeral(
    mut incoming: ResMut<PendingIncoming>,
    mut players: Query<&mut Player>,
    net_mode: Res<NetworkMode>,
) {
    for (eph, _peer) in incoming.ephemeral.drain(..) {
        match eph {
            Ephemeral::AimUpdate { angle, power } => {
                let Some(pid) = net_mode.player_id() else {
                    continue;
                };
                let opponent = 3 - pid;
                for mut player in players.iter_mut() {
                    if player.id == opponent {
                        let initial = if player.id == 1 { 0.0 } else { std::f64::consts::PI };
                        player.angle = angle;
                        player.power = power;
                        player.rel_rot = angle - initial;
                    }
                }
            }
        }
    }
}

/// Broadcast the active player's live aim preview (unreliable, on change only).
fn broadcast_aim(
    net: Res<NetState>,
    net_mode: Res<NetworkMode>,
    turn: Res<TurnState>,
    players: Query<&Player>,
    socket: Option<ResMut<MatchboxSocket>>,
    mut last: Local<(u8, f64, f64)>,
) {
    let Some(pid) = net_mode.player_id() else {
        return;
    };
    if turn.current_player != pid || turn.round_over || turn.firing {
        return;
    }
    let mut cur = None;
    for p in players.iter() {
        if p.id == pid {
            cur = Some((pid, p.angle, p.power));
        }
    }
    let Some(cur) = cur else { return };
    if cur == *last {
        return;
    }
    *last = cur;
    let Some(mut socket) = socket else {
        return;
    };
    gnils_net::broadcast_unreliable(
        &mut socket,
        &net.peers,
        &NetMsg::Ephemeral(Ephemeral::AimUpdate {
            angle: cur.1,
            power: cur.2,
        }),
    );
}

/// Auto-advance the round/game in network mode, deterministically on both
/// peers. After a round ends, wait a few seconds, pick who goes first (lower
/// score; player 1 on ties — same rule as local mode), and start the next
/// round. After game over, reset scores/round and re-seed for a new game.
fn network_auto_advance(
    time: Res<Time>,
    mut turn: ResMut<TurnState>,
    mut players: Query<&mut Player>,
    mut missile_q: Query<(&mut MissileMarker, &mut Visibility), Without<Player>>,
    trail_canvas: Res<TrailCanvas>,
    mut images: ResMut<Assets<Image>>,
    mut next_state: ResMut<NextState<GamePhase>>,
    mut net_seed: ResMut<NetSeed>,
    net_mode: Res<NetworkMode>,
    phase: Res<State<GamePhase>>,
    menu: Res<MenuOpen>,
    mut timer: Local<f32>,
) {
    if !net_mode.is_network() {
        return;
    }
    if *phase.get() != GamePhase::RoundOver || !turn.round_over {
        *timer = 0.0;
        return;
    }
    if menu.open {
        return;
    }

    *timer += time.delta_secs();
    if *timer < 4.0 {
        return;
    }
    *timer = 0.0;

    if turn.game_over {
        for mut player in players.iter_mut() {
            player.score = 0;
        }
        turn.round = 0;
        turn.game_over = false;
        // Deterministic new layout for the next game (same on both peers).
        net_seed.base = net_seed.base.wrapping_add(1);
    }

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
