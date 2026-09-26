mod components;
mod constants;
mod resources;
mod ship_blend;
mod systems;
mod trail;

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::window::MonitorSelection;
use bevy::window::{WindowMode, WindowResolution};

use components::MissileMarker;
use constants::*;
use resources::*;
use systems::lobby::LobbyPlugin;
use systems::network::NetPlugin;
use systems::{collision, input, missile, particles, physics, planet, player, rendering, round, setup};

/// Native opens borderless fullscreen to use the whole monitor; on the web
/// the canvas already fills the viewport and browsers reject startup
/// fullscreen requests without a user gesture.
#[cfg(not(target_arch = "wasm32"))]
const WINDOW_MODE: WindowMode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
#[cfg(target_arch = "wasm32")]
const WINDOW_MODE: WindowMode = WindowMode::Windowed;

/// The phases in which the world is simulated and drawn.
fn in_play(phase: Res<State<GamePhase>>) -> bool {
    matches!(
        phase.get(),
        GamePhase::Aiming | GamePhase::Firing | GamePhase::RoundOver
    )
}

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Slingshot".into(),
                        resolution: WindowResolution::new(
                            WINDOW_WIDTH as u32,
                            WINDOW_HEIGHT as u32,
                        ),
                        resizable: true,
                        mode: WINDOW_MODE,
                        canvas: Some("#game".into()),
                        // Web: let the canvas grow with the browser viewport.
                        // Without this, winit pins the canvas to its initial
                        // size and it never resizes.
                        fit_canvas_to_parent: true,
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    // Web dev servers answer the optional `<asset>.meta`
                    // probe with the index page; parsing that as meta fails
                    // and rejects every asset (gray screen).
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                }),
        )
        // Network plugin (matchbox P2P: sequencing, host election, rooms)
        .add_plugins(NetPlugin)
        // Lobby / main menu plugin
        .add_plugins(LobbyPlugin)
        .insert_resource(Time::<Fixed>::from_hz(gnils_protocol::TICK_HZ))
        .init_state::<GamePhase>()
        .init_resource::<GameSettings>()
        .init_resource::<TurnState>()
        .init_resource::<BounceAnimation>()
        .init_resource::<ParticleSpawnQueue>()
        .init_resource::<MissileImpactQueue>()
        .init_resource::<RoundResult>()
        .init_resource::<MenuOpen>()
        .init_resource::<AimRepeat>()
        // Startup, then the parts that need the assets resource to exist.
        .add_systems(
            Startup,
            (setup::setup_camera, setup::load_assets, auto_join_test),
        )
        .add_systems(
            Startup,
            (
                setup::setup_trail_canvas,
                setup::setup_background,
                setup::setup_players,
                setup::setup_missile,
                setup::setup_zoom_dim,
                setup::setup_ui,
            )
                .after(setup::load_assets),
        )
        // Round setup state (planets + round increment; in network mode the
        // layout is derived deterministically from the shared NetSeed)
        .add_systems(OnEnter(GamePhase::Loading), round::local_new_game_reset)
        .add_systems(
            OnEnter(GamePhase::RoundSetup),
            (planet::spawn_planets, round::round_setup).chain(),
        )
        // Phase transitions.
        .add_systems(
            Update,
            loading_transition_system.run_if(in_state(GamePhase::Loading)),
        )
        .add_systems(
            FixedUpdate,
            (
                firing_done_system.run_if(in_state(GamePhase::Firing)),
                // In network mode the `Sequenced` ShotFired echo sets
                // `turn.firing`; both peers then launch through this same path.
                (missile::fire_missile, fire_transition_system)
                    .chain()
                    .run_if(in_state(GamePhase::Aiming)),
            ),
        )
        // Keyboard input (Update, for reliable key detection).
        .add_systems(
            Update,
            (
                input::aiming_input.run_if(in_state(GamePhase::Aiming)),
                input::round_over_input.run_if(in_state(GamePhase::RoundOver)),
                (input::menu_toggle_input, input::menu_nav_input)
                    .run_if(not(in_state(GamePhase::Loading))),
            ),
        )
        // Simulation, at the fixed timestep. The missile only flies while
        // firing; debris outlives the shot that threw it.
        .add_systems(
            FixedUpdate,
            (
                (physics::missile_gravity, missile::draw_missile_trail)
                    .chain()
                    .run_if(in_state(GamePhase::Firing)),
                collision::missile_collision.run_if(in_state(GamePhase::Firing)),
                (
                    physics::particle_gravity,
                    physics::particle_bounce,
                    collision::particle_collision,
                    particles::cleanup_particles,
                )
                    .chain()
                    .run_if(in_play),
                // Impact handling & particle spawning — in any active state.
                (round::handle_missile_impact, particles::spawn_particles)
                    .chain()
                    .run_if(not(in_state(GamePhase::Loading))),
            ),
        )
        // Rendering, every frame.
        .add_systems(
            Update,
            (
                player::update_player_sprites,
                player::update_ship_explosion,
                player::draw_aim_line,
                player::update_ui_text,
                player::update_turn_banner,
                physics::sync_transforms,
                missile::update_missile_visibility,
                missile::update_missile_ui,
                rendering::update_bounce_animation,
                rendering::update_view_size,
                rendering::update_ui_scale,
                rendering::draw_bounce_border,
                rendering::draw_zoom_view,
                rendering::update_ui_visibility,
                rendering::update_round_overlay,
                rendering::update_round_over_display,
                rendering::update_planet_visibility,
                rendering::update_menu_display,
            ),
        )
        .run();
}

fn loading_transition_system(
    assets: Option<Res<GameAssets>>,
    trail: Option<Res<TrailCanvas>>,
    mut next_state: ResMut<NextState<GamePhase>>,
) {
    if assets.is_some() && trail.is_some() {
        next_state.set(GamePhase::RoundSetup);
    }
}

/// Test hook: if `GNILS_AUTOJOIN=<room>` is set, join the room immediately on
/// startup (skipping the main menu). Used for automated two-instance testing.
fn auto_join_test(mut commands: Commands, mut next: ResMut<NextState<GamePhase>>) {
    if let Ok(room) = std::env::var("GNILS_AUTOJOIN") {
        info!(%room, "GNILS_AUTOJOIN set; auto-joining room");
        gnils_net::open_socket_for(&mut commands, room);
        next.set(GamePhase::Connecting);
    } else if std::env::var("GNILS_AUTOPLAY").is_ok() {
        info!("GNILS_AUTOPLAY set; starting a local game");
        next.set(GamePhase::Loading);
    }
}

fn fire_transition_system(turn: Res<TurnState>, mut next_state: ResMut<NextState<GamePhase>>) {
    if turn.firing {
        info!("Transitioning Aiming -> Firing");
        next_state.set(GamePhase::Firing);
    }
}

fn firing_done_system(
    turn: Res<TurnState>,
    missile_q: Query<&MissileMarker>,
    impact_queue: Res<MissileImpactQueue>,
    mut next_state: ResMut<NextState<GamePhase>>,
) {
    // An impact still waiting to be scored is not a finished shot.
    if !impact_queue.impacts.is_empty() || turn.firing || missile_q.iter().any(|m| m.active) {
        return;
    }

    if turn.round_over {
        info!("Transitioning Firing -> RoundOver");
        next_state.set(GamePhase::RoundOver);
    } else {
        info!(
            "Transitioning Firing -> Aiming (player {})",
            turn.current_player
        );
        next_state.set(GamePhase::Aiming);
    }
}
