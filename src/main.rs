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
                systems::setup::setup_ui,
            )
                .after(systems::setup::load_assets),
        )
        // Round setup state
        .add_systems(
            OnEnter(GamePhase::RoundSetup),
            (systems::planet::spawn_planets, systems::round::round_setup).chain(),
        )
        // Aiming input (Update for reliable key detection)
        .add_systems(
            Update,
            systems::input::aiming_input.run_if(in_state(GamePhase::Aiming)),
        )
        // Fire missile (FixedUpdate for physics setup)
        .add_systems(
            FixedUpdate,
            (
                systems::missile::fire_missile,
                fire_transition_system,
            )
                .chain()
                .run_if(in_state(GamePhase::Aiming)),
        )
        // Firing phase (physics at fixed timestep)
        .add_systems(
            FixedUpdate,
            (
                systems::physics::missile_gravity,
                systems::collision::missile_collision,
                systems::missile::draw_missile_trail,
                systems::physics::particle_gravity,
                systems::collision::particle_collision,
                systems::particles::cleanup_particles,
                firing_done_system,
            )
                .chain()
                .run_if(in_state(GamePhase::Firing)),
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
        // During Firing, particles are handled in the firing chain above
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
                systems::player::draw_aim_line,
                systems::player::update_ui_text,
                systems::physics::sync_transforms,
                systems::missile::update_missile_visibility,
                systems::missile::update_missile_ui,
                systems::rendering::update_bounce_animation,
                systems::rendering::draw_bounce_border,
            ),
        )
        .add_systems(
            Update,
            systems::rendering::update_ui_visibility,
        )
        .run();
}

/// System that transitions from Loading to RoundSetup once GameAssets exists.
fn loading_transition_system(
    assets: Option<Res<GameAssets>>,
    trail: Option<Res<TrailCanvas>>,
    mut next_state: ResMut<NextState<GamePhase>>,
) {
    if assets.is_some() && trail.is_some() {
        next_state.set(GamePhase::RoundSetup);
    }
}

/// System that transitions from Aiming to Firing when the turn state indicates firing.
fn fire_transition_system(turn: Res<TurnState>, mut next_state: ResMut<NextState<GamePhase>>) {
    if turn.firing {
        info!("Transitioning Aiming → Firing");
        next_state.set(GamePhase::Firing);
    }
}

/// System that transitions from Firing back to Aiming or RoundOver when firing ends.
fn firing_done_system(
    turn: Res<TurnState>,
    missile_q: Query<&MissileMarker>,
    impact_queue: Res<MissileImpactQueue>,
    mut next_state: ResMut<NextState<GamePhase>>,
) {
    let missile_active = missile_q.iter().any(|m| m.active);

    // Don't transition if there are pending impacts — let handle_missile_impact process them first
    if !impact_queue.impacts.is_empty() {
        return;
    }

    // Only transition when missile is no longer active AND firing flag is cleared
    if !turn.firing && !missile_active {
        if turn.round_over {
            info!("Transitioning Firing → RoundOver");
            next_state.set(GamePhase::RoundOver);
        } else {
            info!("Transitioning Firing → Aiming (player {})", turn.current_player);
            next_state.set(GamePhase::Aiming);
        }
    }
}
