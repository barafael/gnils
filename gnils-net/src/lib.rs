//! P2P WebRTC networking for Slingshot, ported from the omdurman approach.
//!
//! Architecture: *deterministic lockstep with host-sequenced event sourcing*.
//! Slingshot's simulation (`generate_planets`, `step_gravity`, collision,
//! scoring) is fully deterministic given a seed + the shot inputs, so both
//! peers run the identical simulation and exchange only semantic events:
//!
//! - [`NetMsg::Start`] — the host commits the final roster, seed and settings.
//! - [`GameEvent::ShotFired`] — the active player's shot.
//!
//! Shot events are submitted by a guest as [`NetMsg::Game`], sequenced by the
//! host into [`NetMsg::Sequenced`], and rebroadcast to everyone (including
//! back to the host itself via a loopback queue). Every peer applies an event
//! only in its `Sequenced` form, so all peers observe one canonical, ordered
//! stream.
//!
//! Before the game, the room is a **lobby** (the same approach as
//! chinese-checkers): peers introduce themselves with [`NetMsg::Hello`], claim
//! one of the two player seats with [`NetMsg::Claim`], and the host — the one
//! authority — answers with [`NetMsg::Roster`] broadcasts and the final
//! [`NetMsg::Start`]. Peers without a seat watch the lobby.
//!
//! The socket layer uses `matchbox_socket` directly (the bevy-agnostic core of
//! the matchbox stack), so this crate is independent of the bevy version even
//! though the `bevy_matchbox` integration in the fork tracks a newer bevy. The
//! native message-loop future is spawned on a dedicated tokio runtime (webrtc-rs
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

/// Reliable, ordered channel: lobby traffic, game events, `Sequenced` echoes.
pub const CH_RELIABLE: usize = 0;
/// Unreliable channel: ephemeral display state (aim line previews).
pub const CH_UNRELIABLE: usize = 1;

pub use matchbox_socket::{ChannelConfig, PeerId, PeerState};

// ── Game events (the only things that mutate a running game) ────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum GameEvent {
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

/// Lobby-visible player slot: a connected peer, the name it goes by, and
/// which of the two players it commands once the host grants its claim.
/// `player: None` is a spectator.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    /// Stable within a session; `PeerId`'s string form so it survives postcard.
    pub peer: String,
    pub name: String,
    /// `Some(1)` or `Some(2)` for a seated player, `None` for a spectator.
    pub player: Option<u8>,
}

// ── Wire protocol ───────────────────────────────────────────────────────────

/// Top-level wire envelope.
///
/// Lobby traffic ([`NetMsg::Hello`], [`NetMsg::Claim`], [`NetMsg::Roster`],
/// [`NetMsg::Start`]) and shot events follow the same shape: guests submit,
/// the host is the one authority, and what everyone applies is only ever the
/// host's answer (a `Roster`, a `Start`, or a `Sequenced` shot).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum NetMsg {
    /// Guest -> host: an unsequenced shot submission. Never applied directly.
    Game(GameEvent),
    /// Host -> all: the canonical ordered form. The only one that is applied.
    Sequenced { seq: u32, event: GameEvent },
    /// Any -> all: ephemeral display state on the unreliable channel.
    Ephemeral(Ephemeral),
    /// Introduce yourself on join (and again after a rename).
    Hello { name: String },
    /// Host -> all: the full lobby roster, whenever it changes.
    Roster(Vec<Seat>),
    /// Guest -> host: claim a player seat, or release the one I hold.
    /// `Some(n)` claims player `n` (1 or 2); `None` relinquishes. The host
    /// grants it only if the seat is free — or held by the sender — and the
    /// roster everybody watches reflects the result.
    Claim(Option<u8>),
    /// Host -> all: assignments are final, start playing. Carries the final
    /// roster, the deterministic seed for planet generation, and the host's
    /// settings, so every peer rebuilds the identical game.
    Start {
        seats: Vec<Seat>,
        seed: u64,
        settings: GameSettingsData,
    },
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
    /// The lobby roster: every seated player and spectator. On the host it is
    /// the authority; guests take the host's `Roster` broadcasts verbatim.
    pub seats: Vec<Seat>,
    /// The name this peer goes by in the roster. Survives `leave_room`.
    pub name: String,
    /// Peers we have already sent our [`NetMsg::Hello`] to. Without this a
    /// per-frame greet loop would flood the channel; disconnected peers may
    /// stay listed here — a reconnect arrives with a fresh id anyway.
    pub greeted: Vec<PeerId>,
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
    /// A fresh state that already goes by `name`. Used at startup, and when
    /// leaving a room keeps the player's name.
    pub fn with_name(name: String) -> Self {
        Self {
            name,
            ..Self::default()
        }
    }

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

    /// Is `peer` (a `PeerId`'s string form) this local peer?
    pub fn is_me(&self, peer: &str) -> bool {
        self.my_id.is_some_and(|id| id.to_string() == peer)
    }

    /// This peer's roster entry, if it has one.
    pub fn my_seat(&self) -> Option<&Seat> {
        let me = self.my_id?.to_string();
        self.seats.iter().find(|s| s.peer == me)
    }

    /// Which player (1 or 2) this peer commands, if the host has granted its
    /// claim. `None` for a spectator.
    pub fn my_player(&self) -> Option<u8> {
        self.my_seat()?.player
    }

    /// The name of whoever sits at player `n`, to show in the HUD instead of
    /// "Player n". `None` outside a network game.
    pub fn player_name(&self, player: u8) -> Option<&str> {
        self.seats
            .iter()
            .find(|s| s.player == Some(player))
            .map(|s| s.name.as_str())
    }

    /// Forget everything the current room's socket established — peers, id,
    /// host flag, seats, sequence counters — but keep the player's own name,
    /// which identifies the player rather than the session.
    pub fn leave_room(&mut self) {
        let name = std::mem::take(&mut self.name);
        *self = Self::with_name(name);
    }
}

// ── Room ids ────────────────────────────────────────────────────────────────

/// Why a room name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomIdError {
    Empty,
    TooLong { len: usize },
    /// The offending character, so the message can name it rather than saying
    /// "invalid".
    BadChar(char),
}

impl core::fmt::Display for RoomIdError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RoomIdError::Empty => write!(f, "a room name cannot be empty"),
            RoomIdError::TooLong { len } => write!(
                f,
                "a room name may be at most {} characters, got {len}",
                RoomId::MAX_LEN
            ),
            RoomIdError::BadChar(c) => write!(
                f,
                "'{c}' is not allowed in a room name; use letters, digits, '-' or '_'"
            ),
        }
    }
}

impl core::error::Error for RoomIdError {}

/// The room to join. Peers sharing a room id find each other.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct RoomId(pub String);

impl RoomId {
    /// Long enough for a descriptive name, short enough to type and to read
    /// back to someone over the phone.
    pub const MAX_LEN: usize = 40;

    /// Validate a room name handed over by a share link or typed by a player.
    ///
    /// The name is interpolated into the signaling URL's path, so it is
    /// restricted to characters that need no escaping: letters, digits, `-` and
    /// `_`. That rules out `/`, which would otherwise let a typo silently
    /// redirect the socket to a different path, and whitespace, which is
    /// invisible in a name two people are trying to match.
    ///
    /// ASCII-only, deliberately: the point of a room name here is that two
    /// people can agree on it out of band and type it identically, and
    /// non-ASCII invites homoglyph and normalisation mismatches that look like
    /// the network being broken.
    pub fn parse(name: &str) -> Result<Self, RoomIdError> {
        if name.is_empty() {
            return Err(RoomIdError::Empty);
        }
        if name.chars().count() > Self::MAX_LEN {
            return Err(RoomIdError::TooLong {
                len: name.chars().count(),
            });
        }
        if let Some(c) = name
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
        {
            return Err(RoomIdError::BadChar(c));
        }
        Ok(Self(name.to_string()))
    }
}

/// The alphabet of generated rooms: unambiguous and lowercase, so a room read
/// aloud or copied by hand survives the trip.
const ROOM_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// A freshly generated room id: five characters, roughly 33 million rooms,
/// which is far more than a collision ever needs to be unlikely.
pub fn random_room() -> String {
    let mut seed: u64 = rand::random();
    let mut id = String::new();
    for _ in 0..5 {
        let pick = (seed % ROOM_ALPHABET.len() as u64) as usize;
        seed /= ROOM_ALPHABET.len() as u64;
        id.push(ROOM_ALPHABET[pick] as char);
    }
    id
}

/// Short pet names a player is born with. One word, easy to say at the table;
/// the roster and the HUD mark the player by it.
const PET_NAMES: &[&str] = &[
    "otter", "falcon", "maple", "ember", "comet", "panda", "lynx", "heron", "quail", "gecko",
    "koala", "raven", "tiger", "bison", "crane", "dingo", "eagle", "ibex", "orca", "yak", "hare",
    "moth", "wren", "toad", "newt", "elk", "fox", "owl", "bee", "finch", "mole", "starling",
    "puffin", "badger", "marten", "vole",
];

/// The pet name a session goes by until the player picks their own in the
/// lobby. Drawn once at startup.
pub fn petname() -> String {
    PET_NAMES[rand::random_range(0..PET_NAMES.len())].to_string()
}

// ── Socket resource ─────────────────────────────────────────────────────────

/// A [`WebRtcSocket`] as a Bevy resource. Same shape as bevy_matchbox's
/// `MatchboxSocket`, but implemented here against `matchbox_socket` directly
/// so we don't depend on the fork's bevy integration.
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

/// Build a `MatchboxSocket` for the given room, keeping ICE config and channel
/// layout in one place.
///
/// The room is deliberately *not* completed by the matchmaker (`?next=…`):
/// the lobby decides who plays — the two seated players — and extra peers may
/// watch, so the room accepts everyone who asks.
pub fn build_socket(room: &str) -> MatchboxSocket {
    let url = format!("{SIGNALING_SERVER}/{room}");
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
        .add_reliable_channel() // channel 0: lobby, game events, echoes
        .add_unreliable_channel(); // channel 1: aim previews

    MatchboxSocket::from(builder)
}

/// Register the `RoomId` resource and open a socket for `room` in one call.
/// Deferred via `Commands`, so the socket (and `RoomId`) is available to
/// systems on the following frame.
pub fn open_socket_for(commands: &mut Commands, room: String) {
    commands.insert_resource(RoomId(room.clone()));
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
/// [`NetMsg::Start`].
pub fn new_seed() -> u64 {
    rand::random()
}

/// The room id this instance belongs to. Native: first CLI arg (a fresh
/// generated room otherwise). Wasm: the `?room=` URL parameter — a share
/// link —, generated into the URL if absent.
pub fn room_id() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::wasm_bindgen::JsValue;
        let win = web_sys::window().expect("window always available");
        let href = win.location().href().ok().unwrap_or_default();

        if let Ok(url) = web_sys::Url::new(&href)
            && let Some(id) = url.search_params().get("room")
            && !id.is_empty()
        {
            return id;
        }

        let new_id = random_room();

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
        match std::env::args().nth(1).as_deref() {
            Some(arg) if RoomId::parse(arg).is_ok() => arg.to_string(),
            Some(arg) => {
                warn!("ignoring invalid room name {arg:?}; using a generated room");
                random_room()
            }
            None => "dev-room".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trips() {
        let msg = NetMsg::Sequenced {
            seq: 7,
            event: GameEvent::ShotFired {
                player: 2,
                angle: 1.25,
                power: 200.0,
            },
        };
        let bytes = enc_msg(&msg).expect("encodes");
        let Some(NetMsg::Sequenced { seq, event }) = decode(&bytes) else {
            panic!("decoded to the wrong variant");
        };
        assert_eq!(seq, 7);
        let GameEvent::ShotFired {
            player,
            angle,
            power,
        } = event;
        assert_eq!((player, angle, power), (2, 1.25, 200.0));
    }

    #[test]
    fn lobby_messages_round_trip() {
        let seats = vec![Seat {
            peer: "peer-a".into(),
            name: "otter".into(),
            player: Some(1),
        }];
        for msg in [
            NetMsg::Hello {
                name: "otter".into(),
            },
            NetMsg::Roster(seats.clone()),
            NetMsg::Claim(Some(2)),
            NetMsg::Claim(None),
            NetMsg::Start {
                seats: seats.clone(),
                seed: 42,
                settings: GameSettingsData::default(),
            },
        ] {
            let bytes = enc_msg(&msg).expect("encodes");
            let back = decode(&bytes).expect("decodes");
            assert_eq!(format!("{back:?}"), format!("{msg:?}"));
        }
    }

    #[test]
    fn decoding_garbage_yields_none() {
        assert!(decode(&[0xff, 0xff, 0xff, 0xff]).is_none());
    }

    #[test]
    fn my_seat_and_player_need_the_host_s_grant() {
        let mut net = NetState::default();
        assert!(net.my_seat().is_none());
        assert_eq!(net.my_player(), None);

        net.my_id = Some(PeerId(uuid::Uuid::nil()));
        net.seats = vec![Seat {
            peer: net.my_id.unwrap().to_string(),
            name: "otter".into(),
            player: Some(2),
        }];
        assert_eq!(net.my_player(), Some(2));
        assert_eq!(net.player_name(2), Some("otter"));
        assert_eq!(net.player_name(1), None);
        assert!(net.is_me(&PeerId(uuid::Uuid::nil()).to_string()));
        assert!(!net.is_me("other"));
    }

    /// Leaving the room must forget everything the old room's socket
    /// established but the player's own name, which belongs to the player.
    #[test]
    fn leaving_a_room_forgets_everything_but_the_name() {
        let mut net = NetState {
            is_host: true,
            next_seq: 12,
            last_applied_seq: Some(11),
            name: "ada".into(),
            seats: vec![Seat {
                peer: "p".into(),
                name: "p".into(),
                player: Some(1),
            }],
            greeted: vec![PeerId(uuid::Uuid::nil())],
            ..NetState::default()
        };

        net.leave_room();

        assert_eq!(net.name, "ada", "the player's name is not per-room");
        assert!(!net.is_host, "host status belongs to the old room");
        assert!(net.seats.is_empty(), "seats were assigned by the old host");
        assert!(net.peers.is_empty());
        assert_eq!(net.my_id, None, "the id came from the old socket");
        assert_eq!(net.next_seq, 0);
        assert_eq!(net.last_applied_seq, None, "or the first shot looks stale");
        assert!(net.greeted.is_empty());
    }

    #[test]
    fn generated_rooms_are_five_unambiguous_characters() {
        for _ in 0..50 {
            let room = random_room();
            assert_eq!(room.len(), 5);
            assert!(
                room.chars()
                    .all(|c| ROOM_ALPHABET.contains(&(c as u8))),
                "{room}"
            );
        }
    }

    #[test]
    fn petnames_come_from_the_list() {
        assert!(PET_NAMES.contains(&petname().as_str()));
    }

    #[test]
    fn ordinary_names_are_accepted() {
        for name in ["a", "game", "Room_7", "my-game-2", "ABC123"] {
            assert!(RoomId::parse(name).is_ok(), "{name} should be accepted");
        }
    }

    #[test]
    fn an_empty_name_is_refused() {
        assert_eq!(RoomId::parse(""), Err(RoomIdError::Empty));
    }

    #[test]
    fn an_overlong_name_is_refused() {
        let long = "a".repeat(RoomId::MAX_LEN + 1);
        assert_eq!(
            RoomId::parse(&long),
            Err(RoomIdError::TooLong {
                len: RoomId::MAX_LEN + 1
            })
        );
        // The boundary itself is allowed.
        assert!(RoomId::parse(&"a".repeat(RoomId::MAX_LEN)).is_ok());
    }

    /// A slash would redirect the socket to a different URL path, and
    /// whitespace is invisible in a name two people are trying to match. Both
    /// must be refused rather than silently reinterpreted.
    #[test]
    fn characters_that_would_change_the_url_are_refused() {
        for (name, bad) in [
            ("a/b", '/'),
            ("a b", ' '),
            ("a?b", '?'),
            ("a#b", '#'),
            ("a:b", ':'),
        ] {
            assert_eq!(
                RoomId::parse(name),
                Err(RoomIdError::BadChar(bad)),
                "{name} must be refused"
            );
        }
    }

    #[test]
    fn non_ascii_is_refused() {
        assert_eq!(RoomId::parse("café"), Err(RoomIdError::BadChar('é')));
    }

    #[test]
    fn every_error_explains_itself() {
        assert!(RoomIdError::Empty.to_string().contains("empty"));

        let long = RoomIdError::TooLong { len: 99 }.to_string();
        assert!(long.contains("99"), "{long}");
        assert!(long.contains(&RoomId::MAX_LEN.to_string()), "{long}");

        let bad = RoomIdError::BadChar('/').to_string();
        assert!(bad.contains('/'), "must name the character: {bad}");
    }
}
