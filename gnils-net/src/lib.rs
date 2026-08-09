//! P2P WebRTC networking for Slingshot, ported from the omdurman approach.
//!
//! Architecture: *deterministic lockstep with host-sequenced event sourcing*.
//! Slingshot's simulation (`generate_planets`, `step_gravity`, collision,
//! scoring) is fully deterministic given a seed + the shot inputs, so both
//! peers run the identical simulation and exchange only semantic events:
//!
//! - [`GameEvent::StartGame`] — host commits seed + settings + player ids.
//! - [`GameEvent::ShotFired`] — the active player's shot.
//!
//! Events are submitted by a guest as [`NetMsg::Game`], sequenced by the host
//! into [`NetMsg::Sequenced`], and rebroadcast to everyone (including back to
//! the host itself via a loopback queue). Every peer applies an event only in
//! its `Sequenced` form, so all peers observe one canonical, ordered stream.
//!
//! The socket layer uses `matchbox_socket` directly (the bevy-agnostic core of
//! the matchbox stack), so this crate works with bevy 0.18 even though the
//! `bevy_matchbox` integration in the fork targets bevy 0.19. The native
//! message-loop future is spawned on a dedicated tokio runtime (webrtc-rs
//! needs a real runtime, see `spawn_message_loop`).

use bevy::prelude::*;
use gnils_protocol::GameSettingsData;
use matchbox_socket::{
    MessageLoopFuture, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder,
};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

/// Signaling server URL. Overridable at build time via `MATCHBOX_SERVER`.
pub const SIGNALING_SERVER: &str = if let Some(s) = option_env!("MATCHBOX_SERVER") {
    s
} else {
    "wss://omdurman-matchbox.fly.dev"
};

/// Reliable, ordered channel: game events, `Sequenced` echoes.
pub const CH_RELIABLE: usize = 0;
/// Unreliable channel: ephemeral display state (aim line previews).
pub const CH_UNRELIABLE: usize = 1;

pub use matchbox_socket::{ChannelConfig, PeerId, PeerState};

// ── Game events (the only things that mutate the world) ─────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GameEvent {
    /// Host-committed game start. Carries the deterministic seed, the host's
    /// settings, and the PeerId -> player-id (1 or 2) binding. Player 1 is the
    /// lowest-sorted PeerId in the room. Recorded + replayed, so a late joiner
    /// learns everything it needs to rebuild the world deterministically.
    StartGame {
        seed: u64,
        settings: GameSettingsData,
        assignments: Vec<(PeerId, u8)>,
    },
    /// The active player fired. Both peers launch a missile with these exact
    /// parameters and derive the outcome (flight, collision, scoring, turn
    /// order) locally and identically.
    ShotFired {
        player: u8,
        angle: f64,
        power: f64,
    },
}

/// Display-only state that never affects the simulation. Sent on the
/// unreliable channel; the latest value supersedes any in-flight one.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Ephemeral {
    /// The active player's live aim preview, so the opponent's ship + aim line
    /// update on the other screen.
    AimUpdate { angle: f64, power: f64 },
}

// ── Wire protocol ───────────────────────────────────────────────────────────

/// Top-level wire envelope. A non-host submits [`NetMsg::Game`] to the host
/// only; the host assigns the next canonical sequence number and rebroadcasts
/// it as [`NetMsg::Sequenced`] to every peer, including itself via the
/// loopback queue. Only the `Sequenced` form is applied to the world.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum NetMsg {
    Game(GameEvent),
    Sequenced { seq: u32, event: GameEvent },
    Ephemeral(Ephemeral),
}

/// Encode a `NetMsg` for the wire. Returns `None` if encoding fails or would
/// produce a zero-length payload. WebRTC data channels may silently drop a
/// zero-byte payload, so we refuse to emit one entirely.
pub fn enc_msg(msg: &NetMsg) -> Option<Box<[u8]>> {
    match postcard::to_allocvec(msg) {
        Ok(v) if !v.is_empty() => Some(v.into_boxed_slice()),
        Ok(_) => {
            error!("postcard produced an empty NetMsg encoding; dropping");
            None
        }
        Err(e) => {
            error!("postcard encode failed: {e}");
            None
        }
    }
}

pub fn decode(raw: &[u8]) -> Option<NetMsg> {
    postcard::from_bytes(raw)
        .inspect_err(|e| warn!("matchbox decode error: {e}"))
        .ok()
}

// ── Net state ───────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct NetState {
    pub peers: Vec<PeerId>,
    pub my_id: Option<PeerId>,
    pub is_host: bool,
    /// Host-only: the next canonical sequence number to assign. Meaningless on
    /// guests.
    pub next_seq: u32,
    /// Highest sequence number applied locally, so a duplicate delivery (same
    /// or lower `seq`) is never applied twice. `None` until the first event.
    pub last_applied_seq: Option<u32>,
    /// All peers including `my_id`, in canonical sorted order.
    sorted_all: Vec<PeerId>,
}

impl NetState {
    /// Rebuild `sorted_all` from `peers` + `my_id`. Call after any mutation.
    pub fn refresh_sorted(&mut self) {
        self.sorted_all.clear();
        self.sorted_all.extend(self.peers.iter().copied());
        if let Some(me) = self.my_id {
            self.sorted_all.push(me);
        }
        self.sorted_all.sort();
    }

    /// Canonical sorted list of all peers including the local player.
    pub fn sorted_all(&self) -> &[PeerId] {
        &self.sorted_all
    }

    /// The canonical host: the lowest-sorted peer id across everyone. Re-derived
    /// on every peer change, so a guest is promoted automatically if the host
    /// disconnects.
    pub fn host_id(&self) -> Option<PeerId> {
        self.sorted_all.first().copied()
    }

    /// The 1-based player id (1 or 2) assigned to `peer` in a 2-player game:
    /// the lowest-sorted PeerId is Player 1, the other is Player 2.
    pub fn player_id_for(&self, peer: PeerId) -> Option<u8> {
        self.sorted_all
            .binary_search(&peer)
            .ok()
            .map(|i| i as u8 + 1)
    }
}

// ── Socket resource ─────────────────────────────────────────────────────────

/// A [`WebRtcSocket`] as a Bevy resource. Same shape as bevy_matchbox's
/// `MatchboxSocket`, but implemented here against `matchbox_socket` directly
/// so we don't depend on the fork's bevy 0.19 integration.
#[derive(Resource)]
pub struct MatchboxSocket(WebRtcSocket);

impl Deref for MatchboxSocket {
    type Target = WebRtcSocket;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MatchboxSocket {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<WebRtcSocketBuilder> for MatchboxSocket {
    fn from(builder: WebRtcSocketBuilder) -> Self {
        Self::from(builder.build())
    }
}

impl From<(WebRtcSocket, MessageLoopFuture)> for MatchboxSocket {
    fn from((socket, message_loop_fut): (WebRtcSocket, MessageLoopFuture)) -> Self {
        spawn_message_loop(message_loop_fut);
        MatchboxSocket(socket)
    }
}

/// Spawn the matchbox message-loop future so it keeps running for the lifetime
/// of the socket.
///
/// On native, `webrtc-rs` (used by `matchbox_socket`) depends on a live tokio
/// runtime for timers and I/O. `matchbox_socket` wraps its handshake futures
/// in `async-compat`, which enters a global single-threaded tokio context —
/// but that fallback runtime's timer is not sufficient for webrtc-rs 0.17's
/// DTLS/SCTP handshake to complete when polled from Bevy's `IoTaskPool`. The
/// result: ICE connects but data channels never open. Spawning directly on a
/// real multi-threaded tokio runtime fixes this.
///
/// On WASM there is no tokio and no webrtc-rs (the browser provides WebRTC),
/// so we fall back to `IoTaskPool::spawn(..).detach()` as before.
#[cfg(not(target_arch = "wasm32"))]
fn spawn_message_loop(fut: MessageLoopFuture) {
    use std::sync::OnceLock;
    use tokio::runtime::Runtime;
    use tokio::task::JoinHandle;

    /// A global multi-threaded tokio runtime dedicated to the matchbox message
    /// loop. Created once, reused for every socket (reconnects, etc.).
    static MATCHBOX_RUNTIME: OnceLock<Runtime> = OnceLock::new();

    let runtime = MATCHBOX_RUNTIME.get_or_init(|| {
        // webrtc-rs uses rustls for DTLS. rustls 0.23 requires a process-level
        // CryptoProvider to be installed before any config is built.
        let _ = rustls::crypto::ring::default_provider().install_default();

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("matchbox")
            .build()
            .expect("failed to build matchbox tokio runtime")
    });

    // Detach the JoinHandle so it runs in the background. The runtime lives for
    // 'static and the task completes when the socket closes.
    let _handle: JoinHandle<()> = runtime.spawn(async move {
        let _ = fut.await;
    });
}

#[cfg(target_arch = "wasm32")]
fn spawn_message_loop(fut: MessageLoopFuture) {
    use bevy::tasks::IoTaskPool;
    IoTaskPool::get().spawn(fut).detach();
}

#[derive(Resource)]
pub struct RoomId(pub(crate) String);

impl RoomId {
    pub fn new(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Build a `MatchboxSocket` for the given room, keeping ICE config and channel
/// layout in one place.
pub fn build_socket(room: &str) -> MatchboxSocket {
    // `?next=2` tells the matchbox_server matchmaker to complete the room once
    // two players have joined (and then clear it for the next pair).
    let url = format!("{SIGNALING_SERVER}/{room}?next=2");
    info!(%room, %url, "opening matchbox socket");

    let ice_config = RtcIceServerConfig {
        urls: vec![
            "stun:stun.l.google.com:19302".to_string(),
            "stun:stun1.l.google.com:19302".to_string(),
        ],
        username: None,
        credential: None,
    };

    let builder = WebRtcSocketBuilder::new(&url)
        .ice_server(ice_config)
        .reconnect_attempts(None) // unlimited reconnection attempts
        .add_reliable_channel() // channel 0: game events, sequenced echoes
        .add_unreliable_channel(); // channel 1: aim previews

    MatchboxSocket::from(builder)
}

/// Register the `RoomId` resource and open a socket for `room` in one call.
/// Deferred via `Commands`, so the socket (and `RoomId`) is available to
/// systems on the following frame.
pub fn open_socket_for(commands: &mut Commands, room: String) {
    commands.insert_resource(RoomId::new(room.clone()));
    commands.insert_resource(build_socket(&room));
}

/// Broadcast an ephemeral message to every peer on the unreliable channel.
/// Send failures are silently dropped — the next sample supersedes.
pub fn broadcast_unreliable(socket: &mut MatchboxSocket, peers: &[PeerId], msg: &NetMsg) {
    if peers.is_empty() {
        return;
    }
    let Some(encoded) = enc_msg(msg) else {
        return;
    };
    let channel = socket.channel_mut(CH_UNRELIABLE);
    for &peer in peers {
        let _ = channel.try_send(encoded.clone(), peer);
    }
}

/// Random 64-bit seed for a fresh game. Host-generated, committed in
/// [`GameEvent::StartGame`].
pub fn new_seed() -> u64 {
    rand::random()
}

/// The room id this instance belongs to. Native: first CLI arg (default
/// `dev-room`). Wasm: the `?room=` URL parameter, generated into the URL if
/// absent.
pub fn room_id() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::wasm_bindgen::JsValue;
        let win = web_sys::window().expect("window always available");
        let href = win.location().href().ok().unwrap_or_default();

        if let Ok(url) = web_sys::Url::new(&href) {
            if let Some(id) = url.search_params().get("room") {
                if !id.is_empty() {
                    return id;
                }
            }
        }

        let new_id = format!("{:08x}", new_seed() as u32);

        if let Ok(url) = web_sys::Url::new(&href) {
            url.search_params().set("room", &new_id);
            if let Ok(history) = win.history() {
                let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&url.href()));
            }
        }

        new_id
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "dev-room".to_string())
    }
}
