mod components;
mod constants;
mod events;
mod resources;
mod ship_blend;
mod systems;
mod trail;

use bevy::prelude::*;
use bevy::window::WindowResolution;

use components::MissileMarker;
use constants::*;
use resources::*;
use systems::lobby::LobbyPlugin;
use systems::network::GnilsClientNetPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Slingshot".into(),
                resolution: WindowResolution::new(WINDOW_WIDTH as u32, WINDOW_HEIGHT as u32),
                resizable: false,
                ..default()
            }),
            ..default()
        }))
        // Network plugin (registers Lightyear client, channels, messages)
        .add_plugins(GnilsClientNetPlugin)
        // Lobby / main menu plugin
        .add_plugins(LobbyPlugin)
        // Fixed timestep at 30 Hz to match original physics
        .insert_resource(Time::<Fixed>::from_hz(FPS))
        // Game state
        .init_state::<GamePhase>()
        // Resources
        .insert_resource(GameSettings::default())
        .insert_resource(TurnState::default())
        .insert_resource(BounceAnimation::default())
        .insert_resource(ParticleSpawnQueue::default())
        .insert_resource(MissileImpactQueue::default())
        .insert_resource(RoundResult::default())
        .insert_resource(MenuOpen::default())
        .insert_resource(NetworkMode::default())
        .insert_resource(JoinAddress::default())
        .insert_resource(LobbyMenu::default())
        // Startup systems (run once)
        .add_systems(
            Startup,
            (systems::setup::setup_camera, systems::setup::load_assets),
        )
        // Deferred startup (needs assets resource to exist)
        .add_systems(
            Startup,
            (
                systems::setup::setup_trail_canvas,
                systems::setup::setup_background,
                systems::setup::setup_players,
                systems::setup::setup_missile,
                systems::setup::setup_zoom_dim,
                systems::setup::setup_ui,
            )
                .after(systems::setup::load_assets),
        )
        // Round setup state (local only; in network mode the server drives RoundSetup via message)
        .add_systems(
            OnEnter(GamePhase::RoundSetup),
            (systems::planet::spawn_planets, systems::round::round_setup)
                .chain()
                .run_if(resource_equals(NetworkMode::Local)),
        )
        // Aiming input (Update for reliable key detection)
        .add_systems(
            Update,
            systems::input::aiming_input.run_if(in_state(GamePhase::Aiming)),
        )
        // Fire missile (FixedUpdate for physics setup) — local only; server drives in network mode
        .add_systems(
            FixedUpdate,
            (systems::missile::fire_missile, fire_transition_system)
                .chain()
                .run_if(in_state(GamePhase::Aiming).and(resource_equals(NetworkMode::Local))),
        )
        // Firing phase (physics at fixed timestep)
        .add_systems(
            FixedUpdate,
            (
                systems::physics::missile_gravity,
                systems::missile::draw_missile_trail,
                systems::physics::particle_gravity,
                systems::collision::particle_collision,
                systems::particles::cleanup_particles,
            )
                .chain()
                .run_if(in_state(GamePhase::Firing)),
        )
        // Collision detection — local only; server handles collision in network mode
        .add_systems(
            FixedUpdate,
            systems::collision::missile_collision
                .run_if(in_state(GamePhase::Firing).and(resource_equals(NetworkMode::Local))),
        )
        // Firing → Aiming/RoundOver transition — local only; server drives transitions in network mode
        .add_systems(
            FixedUpdate,
            firing_done_system
                .run_if(in_state(GamePhase::Firing).and(resource_equals(NetworkMode::Local))),
        )
        // Impact handling & particle spawning — runs in any active state
        .add_systems(
            FixedUpdate,
            (
                systems::round::handle_missile_impact,
                systems::particles::spawn_particles,
            )
                .chain()
                .run_if(not(in_state(GamePhase::Loading))),
        )
        // Particle physics during non-Firing states (Aiming, RoundOver)
        .add_systems(
            FixedUpdate,
            (
                systems::physics::particle_gravity,
                systems::collision::particle_collision,
                systems::particles::cleanup_particles,
            )
                .chain()
                .run_if(in_state(GamePhase::Aiming).or(in_state(GamePhase::RoundOver))),
        )
        // Round over input (Update for reliable key detection)
        .add_systems(
            Update,
            systems::input::round_over_input.run_if(in_state(GamePhase::RoundOver)),
        )
        // Menu input runs in any active state
        .add_systems(
            Update,
            (
                systems::input::menu_toggle_input,
                systems::input::menu_nav_input,
            )
                .run_if(not(in_state(GamePhase::Loading))),
        )
        // Loading → RoundSetup transition
        .add_systems(
            Update,
            loading_transition_system.run_if(in_state(GamePhase::Loading)),
        )
        // Update phase (runs every frame for rendering) - split to stay within 8-tuple limit
        .add_systems(
            Update,
            (
                systems::player::update_player_sprites,
                systems::player::update_ship_explosion,
                systems::player::draw_aim_line,
                systems::player::update_ui_text,
                systems::physics::sync_transforms,
                systems::missile::update_missile_visibility,
                systems::missile::update_missile_ui,
                systems::rendering::update_bounce_animation,
            ),
        )
        .add_systems(
            Update,
            (
                systems::rendering::draw_bounce_border,
                systems::rendering::draw_zoom_view,
                systems::rendering::update_ui_visibility,
                systems::rendering::update_round_overlay,
                systems::rendering::update_round_over_display,
                systems::rendering::update_planet_visibility,
                systems::rendering::update_menu_display,
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

fn fire_transition_system(turn: Res<TurnState>, mut next_state: ResMut<NextState<GamePhase>>) {
    if turn.firing {
        info!("Transitioning Aiming → Firing");
        next_state.set(GamePhase::Firing);
    }
}

fn firing_done_system(
    turn: Res<TurnState>,
    missile_q: Query<&MissileMarker>,
    impact_queue: Res<MissileImpactQueue>,
    mut next_state: ResMut<NextState<GamePhase>>,
) {
    let missile_active = missile_q.iter().any(|m| m.active);

    if !impact_queue.impacts.is_empty() {
        return;
    }

    if !turn.firing && !missile_active {
        if turn.round_over {
            info!("Transitioning Firing → RoundOver");
            next_state.set(GamePhase::RoundOver);
        } else {
            info!(
                "Transitioning Firing → Aiming (player {})",
                turn.current_player
            );
            next_state.set(GamePhase::Aiming);
        }
    }
}
