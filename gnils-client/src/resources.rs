use bevy::prelude::*;
use gnils_protocol::GameSettingsData;


// ── Game phases ────────────────────────────────────────────────────────────

#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum GamePhase {
    /// Splash / main menu shown on startup.
    #[default]
    MainMenu,
    /// Connecting to server (network mode only).
    Connecting,
    /// Network lobby: greeted the room; claim a seat, wait for the host.
    WaitingForOpponent,
    /// Loading assets; entered after GameStart or local New Game.
    Loading,
    RoundSetup,
    Aiming,
    Firing,
    RoundOver,
}

// ── Network mode ───────────────────────────────────────────────────────────

/// Whether this session is local hotseat or networked.
#[derive(Resource, Default, Clone, PartialEq, Eq, Debug)]
pub enum NetworkMode {
    #[default]
    Local,
    /// Connected to server; `player_id` is 1 or 2.
    Network { player_id: u8 },
}

impl NetworkMode {
    pub fn is_network(&self) -> bool {
        self.player_id().is_some()
    }
    pub fn player_id(&self) -> Option<u8> {
        match self {
            NetworkMode::Local => None,
            NetworkMode::Network { player_id } => Some(*player_id),
        }
    }
}

/// Room id for the P2P matchbox room we join or host.
#[derive(Resource, Default)]
pub struct JoinRoom(pub TextField);

/// Draft text while the name editor is open.
#[derive(Resource, Default)]
pub struct NameDraft(pub TextField);

/// A single-line editor state: the value plus a caret position, so arrows,
/// Home/End and Delete edit the middle of the text instead of only the tail.
#[derive(Default, Clone)]
pub struct TextField {
    value: String,
    /// Caret position, counted in characters (not bytes).
    cursor: usize,
}

impl TextField {
    /// Upper bound for any single field; the join field needs room for a
    /// pasted URL, a name only for a name.
    pub const MAX_LEN: usize = 120;

    pub fn text(&self) -> &str {
        &self.value
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn char_len(&self) -> usize {
        self.value.chars().count()
    }

    pub fn set(&mut self, s: &str) {
        self.value = s.chars().take(TextField::MAX_LEN).collect();
        self.end();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        if self.cursor < self.char_len() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.char_len();
    }

    /// The text before and after the caret.
    fn split(&self) -> (String, String) {
        (
            self.value.chars().take(self.cursor).collect(),
            self.value.chars().skip(self.cursor).collect(),
        )
    }

    /// Insert text at the caret, keeping the field within `max_chars`.
    pub fn insert(&mut self, s: &str, max_chars: usize) {
        let (head, tail) = self.split();
        let room = max_chars.saturating_sub(self.char_len());
        let added: String = s.chars().take(room).collect();
        self.cursor += added.chars().count();
        self.value = head + &added + &tail;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.left();
            self.delete();
        }
    }

    pub fn delete(&mut self) {
        let (head, tail) = self.split();
        let mut rest = tail.chars();
        if rest.next().is_some() {
            self.value = head + rest.as_str();
        }
    }

    /// Delete the word before the caret (Ctrl+Backspace).
    pub fn backspace_word(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        self.value = chars[..i].iter().collect::<String>()
            + &chars[self.cursor..].iter().collect::<String>();
        self.cursor = i;
    }

    /// The value with the caret glyph drawn at its position.
    pub fn render(&self, caret: &str) -> String {
        let (head, tail) = self.split();
        head + caret + &tail
    }
}

/// Lobby menu state.
#[derive(Resource, Default)]
pub struct LobbyMenu {
    pub selected: usize,
    pub screen: LobbyScreen,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LobbyScreen {
    #[default]
    Main,
    NetworkSub,
    Join,
    /// The shared room lobby: roster, seat claims, host start.
    Room,
    /// The name editor (reachable from the network menu and the room lobby).
    Name,
    Settings,
    Help,
}

/// The lobby's one-line status: the last thing that happened in the room
/// ("otter took Player 2.", "Invite link copied.").
#[derive(Resource, Default)]
pub struct LobbyStatus(pub String);

// ── Game settings ──────────────────────────────────────────────────────────

/// The game's rules and presentation options.
///
/// Everything the two peers must agree on lives in `shared`, which travels
/// verbatim in `NetMsg::Start`; the struct derefs to it, so a rule reads as
/// `settings.bounce` either way. Fields outside `shared` are this machine's
/// business alone.
#[derive(Resource)]
pub struct GameSettings {
    pub shared: GameSettingsData,
    pub fullscreen: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            // The shared rules are the original's defaults exactly, down
            // to its unlimited round count.
            shared: GameSettingsData::default(),
            fullscreen: true,
        }
    }
}

impl std::ops::Deref for GameSettings {
    type Target = GameSettingsData;
    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

impl std::ops::DerefMut for GameSettings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shared
    }
}

/// State of the in-game settings menu (opened with Escape during play).
#[derive(Resource, Default)]
pub struct MenuOpen {
    pub open: bool,
    pub selected: usize,
}

impl MenuOpen {
    /// The row the cursor is on.
    pub fn item(&self) -> MenuItem {
        MENU_ITEMS.get(self.selected).copied().unwrap_or(MENU_ITEMS[0])
    }
}

/// One row of the in-game settings menu. The renderer draws these and
/// `menu_nav_input` acts on them, so neither can drift from the other: the
/// order lives in [`MENU_ITEMS`] alone, and a new row is a compile error
/// until it has both a label and an action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuItem {
    Resume,
    NewGame,
    MainMenu,
    Bounce,
    FixedPower,
    Invisible,
    Particles,
    MaxPlanets,
    MaxBlackholes,
    Rounds,
    Fullscreen,
    Random,
}

/// The settings menu, top to bottom. `MenuOpen::selected` indexes it.
pub const MENU_ITEMS: [MenuItem; 12] = [
    MenuItem::Resume,
    MenuItem::NewGame,
    MenuItem::MainMenu,
    MenuItem::Bounce,
    MenuItem::FixedPower,
    MenuItem::Invisible,
    MenuItem::Particles,
    MenuItem::MaxPlanets,
    MenuItem::MaxBlackholes,
    MenuItem::Rounds,
    MenuItem::Fullscreen,
    MenuItem::Random,
];

impl MenuItem {
    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Resume => "Resume Game",
            MenuItem::NewGame => "New Game",
            MenuItem::MainMenu => "Main Menu",
            MenuItem::Bounce => "Bounce",
            MenuItem::FixedPower => "Fixed Power",
            MenuItem::Invisible => "Invisible Planets",
            MenuItem::Particles => "Particles",
            MenuItem::MaxPlanets => "Max Planets",
            MenuItem::MaxBlackholes => "Max Blackholes",
            MenuItem::Rounds => "Rounds",
            MenuItem::Fullscreen => "Fullscreen",
            MenuItem::Random => "Random Mode",
        }
    }

    /// What this row currently reads, for the rows that show a value. The
    /// three actions at the top show none.
    pub fn value(self, settings: &GameSettings) -> Option<String> {
        let on_off = |v: bool| if v { "ON" } else { "OFF" }.to_string();
        Some(match self {
            MenuItem::Resume | MenuItem::NewGame | MenuItem::MainMenu => return None,
            MenuItem::Bounce => on_off(settings.bounce),
            MenuItem::FixedPower => on_off(settings.fixed_power),
            MenuItem::Invisible => on_off(settings.invisible),
            MenuItem::Particles => on_off(settings.particles_enabled),
            MenuItem::MaxPlanets => settings.max_planets.to_string(),
            MenuItem::MaxBlackholes => settings.max_blackholes.to_string(),
            MenuItem::Rounds => crate::systems::lobby::rounds_label(settings.max_rounds),
            MenuItem::Fullscreen => on_off(settings.fullscreen),
            MenuItem::Random => on_off(settings.random),
        })
    }
}

// ── Turn state ─────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct TurnState {
    pub current_player: u8,
    pub last_player: u8,
    pub round: u32,
    pub round_over: bool,
    pub firing: bool,
    pub show_round: f64,
    pub show_planets: f64,
    pub game_over: bool,
}

impl Default for TurnState {
    fn default() -> Self {
        Self {
            current_player: 1,
            last_player: 1,
            round: 0,
            round_over: false,
            firing: false,
            show_round: 100.0,
            show_planets: 0.0,
            game_over: false,
        }
    }
}

impl TurnState {
    pub fn other_player(&self) -> u8 {
        3 - self.last_player
    }
}

/// Seconds until the network game auto-advances to the next round (or a new
/// game), shown as a countdown. `None` while no advance is pending.
#[derive(Resource, Default)]
pub struct RoundAdvance(pub Option<f32>);

// ── Asset / rendering resources ────────────────────────────────────────────

/// The canvas the missile trail is painted onto, one pixel per world unit,
/// centred on the origin.
///
/// It covers everything the camera can see, not just the 4:3 playfield: the
/// window may be any shape, and a line drawn past the canvas edge is clipped
/// away, which would leave the trail ending in mid-air.
#[derive(Resource)]
pub struct TrailCanvas {
    pub image_handle: Handle<Image>,
    pub size: UVec2,
}

impl TrailCanvas {
    /// A world position as a pixel position on the canvas (pixels run Y-down
    /// from the top-left; the world is Y-up from the centre).
    pub fn to_pixel(&self, pos: (f64, f64)) -> (f64, f64) {
        let half = self.size.as_vec2() / 2.0;
        (pos.0 + half.x as f64, half.y as f64 - pos.1)
    }
}

#[derive(Resource)]
pub struct BounceAnimation {
    pub count: f32,
    pub inc: f32,
}

impl Default for BounceAnimation {
    fn default() -> Self {
        Self {
            count: 255.0,
            inc: 7.0,
        }
    }
}

#[derive(Resource)]
pub struct GameAssets {
    pub font: Handle<Font>,
    pub backdrop: Handle<Image>,
    pub red_ship: Handle<Image>,
    pub blue_ship: Handle<Image>,
    pub shot: Handle<Image>,
    pub explosion: Handle<Image>,
    pub explosion_10: Handle<Image>,
    pub explosion_5: Handle<Image>,
    pub planets: [Handle<Image>; 8],
}

/// Pre-allocated images that receive the per-frame blended ship sprite.
#[derive(Resource)]
pub struct BlendedShipImages {
    pub handles: [Handle<Image>; 2],
}

/// Result of the last round (for end-round message display).
#[derive(Resource, Default, Clone)]
pub struct RoundResult {
    pub hit_player: u8,
    pub self_hit: bool,
    pub hit_score: i32,
    pub quick_bonus: i32,
    pub power_penalty: i32,
    pub total_score: i32,
    pub message: String,
}

/// Deterministic RNG base seed for network games. The per-round seed is
/// `base ^ round` (plus a salt for the player-Y draw), so both peers generate
/// identical planets and layouts without exchanging them.
#[derive(Resource, Default)]
pub struct NetSeed {
    pub base: u64,
}

/// Queued particle spawn requests.
#[derive(Resource, Default)]
pub struct ParticleSpawnQueue {
    pub requests: Vec<ParticleSpawnRequest>,
}

pub struct ParticleSpawnRequest {
    pub pos: Vec2,
    pub count: u32,
    pub size: u8,
}

/// Queued missile impacts.
#[derive(Resource, Default)]
pub struct MissileImpactQueue {
    pub impacts: Vec<MissileImpact>,
}

pub struct MissileImpact {
    pub pos: Vec2,
    pub hit_type: HitType,
}

/// What a missile ran into.
#[derive(Debug, Clone, Copy)]
pub enum HitType {
    Planet,
    Blackhole,
    Ship(u8),
}

/// Per-key auto-repeat timers for the four aiming keys, in the order of
/// `AIM_KEYS` (matching the original's `pygame.key.set_repeat(250, 30)`
/// discrete-repeat model).
#[derive(Resource, Default)]
pub struct AimRepeat(pub [KeyRepeatTimer; 4]);

#[derive(Default, Clone, Copy)]
pub struct KeyRepeatTimer {
    /// `None` = key not held; `Some(secs)` = countdown until next step fires.
    pub delay: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trail_canvas_maps_the_world_onto_its_centre() {
        // A 16:9 window: the camera shows well past the 4:3 playfield's
        // x = +/-400, and the canvas has to stretch with it.
        let canvas = TrailCanvas {
            image_handle: Handle::default(),
            size: UVec2::new(1068, 600),
        };
        assert_eq!(canvas.to_pixel((0.0, 0.0)), (534.0, 300.0));

        // Y runs the other way: the top of the world is pixel row zero.
        let (_, top) = canvas.to_pixel((0.0, 300.0));
        assert_eq!(top, 0.0);

        // The far corner of a wide view still lands on the canvas. Fixed at
        // 800x600 this clipped, and the trail stopped in mid-air.
        let (x, y) = canvas.to_pixel((-533.0, -299.0));
        assert!((0.0..1068.0).contains(&x), "x off canvas: {x}");
        assert!((0.0..600.0).contains(&y), "y off canvas: {y}");
    }
}
