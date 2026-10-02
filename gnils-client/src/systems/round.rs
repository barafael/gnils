use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use gnils_protocol::compute_shot_score;
use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

// ── Shared round bookkeeping ───────────────────────────────────────────────

/// Everything a round or game reset touches, bundled so the systems that
/// trigger one need a single parameter instead of five.
///
/// `GameSettings` stays out on purpose: some callers hold it mutably (the
/// settings menu), which would clash with a read-only copy in here, so the
/// methods take it by reference instead.
#[derive(SystemParam)]
pub struct RoundReset<'w, 's> {
    pub turn: ResMut<'w, TurnState>,
    pub players: Query<'w, 's, &'static mut Player>,
    pub missiles:
        Query<'w, 's, (&'static mut MissileMarker, &'static mut Visibility), Without<Player>>,
    particles: Query<'w, 's, Entity, With<ParticleMarker>>,
    spawn_queue: ResMut<'w, ParticleSpawnQueue>,
    commands: Commands<'w, 's>,
    trail: Res<'w, TrailCanvas>,
    images: ResMut<'w, Assets<Image>>,
}

impl RoundReset<'_, '_> {
    /// Both players' scores, as `(player 1, player 2)`.
    pub fn scores(&self) -> (i32, i32) {
        scores(self.players.iter())
    }

    /// Who opens the next round: whoever is behind on score, and on a tie the
    /// player who did not just shoot.
    pub fn first_shooter(&self) -> u8 {
        let (p1, p2) = self.scores();
        match p1.cmp(&p2) {
            std::cmp::Ordering::Less => 1,
            std::cmp::Ordering::Greater => 2,
            std::cmp::Ordering::Equal => self.turn.other_player(),
        }
    }

    /// Wipe the scoreboard: a finished game leaves nothing behind.
    pub fn reset_scores(&mut self) {
        for mut player in self.players.iter_mut() {
            player.score = 0;
        }
        self.turn.round = 0;
        self.turn.game_over = false;
    }

    /// Prepare a fresh round: clear the trail, re-arm the ships, stow the
    /// missile.
    pub fn new_round(&mut self, settings: &GameSettings) {
        if let Some(image) = self.images.get_mut(&self.trail.image_handle) {
            crate::trail::clear_trail(image.into_inner());
        }

        // A round opens on empty space: debris from the last one must not
        // still be drifting past the new planets.
        for entity in self.particles.iter() {
            self.commands.entity(entity).despawn();
        }
        self.spawn_queue.requests.clear();

        self.turn.round_over = false;
        self.turn.firing = false;
        self.turn.show_round = 100.0;

        for mut player in self.players.iter_mut() {
            player.power = if settings.fixed_power {
                FIXED_POWER_VALUE
            } else {
                100.0
            };
            player.shot = false;
            player.attempts = 0;
            player.explosion_progress = 0.0;
            player.rel_rot = 0.0;
            player.angle = initial_angle(player.id);
        }

        for (mut marker, mut vis) in self.missiles.iter_mut() {
            marker.active = false;
            *vis = Visibility::Hidden;
        }
    }

    /// Full game reset: zero scores and rounds, then prepare round one (used
    /// by local "New Game" and when a network `Start` applies).
    pub fn game_start(&mut self, settings: &GameSettings) {
        self.reset_scores();
        self.new_round(settings);
        self.turn.show_planets = show_planets_for(settings);
    }

    /// Move on after a round ended: start a new game if the last one is over,
    /// then hand the opening shot to `first_shooter`.
    pub fn advance_round(&mut self, settings: &GameSettings) {
        if self.turn.game_over {
            self.reset_scores();
        }
        self.turn.current_player = self.first_shooter();
        self.new_round(settings);
    }
}

/// Both players' scores, as `(player 1, player 2)`.
pub fn scores<'a>(players: impl IntoIterator<Item = &'a Player>) -> (i32, i32) {
    let (mut p1, mut p2) = (0, 0);
    for player in players {
        if player.id == 1 {
            p1 = player.score;
        } else {
            p2 = player.score;
        }
    }
    (p1, p2)
}

/// How long the planets stay visible at the start of a round: a fade-in in
/// invisible-planets mode, nothing at all otherwise.
fn show_planets_for(settings: &GameSettings) -> f64 {
    if settings.invisible { 100.0 } else { 0.0 }
}

/// The RNG that draws a round's layout. Networked peers must draw the same
/// numbers, so theirs comes from the shared seed; a local game gets a fresh
/// one.
pub fn layout_rng(net_mode: &NetworkMode, net_seed: Option<&NetSeed>, round: u32) -> Box<dyn Rng> {
    if net_mode.is_network() {
        let base = net_seed.map_or(0, |s| s.base);
        Box::new(StdRng::seed_from_u64(base ^ round as u64))
    } else {
        Box::new(rand::rng())
    }
}

// ── Systems ────────────────────────────────────────────────────────────────

/// Main-menu "New Game": wipe whatever a previous game left behind — scores,
/// round counter, game-over flag, ship angles, the trail. Network starts
/// reset through their own apply path (`Start`) instead.
pub fn local_new_game_reset(
    net_mode: Res<NetworkMode>,
    settings: Res<GameSettings>,
    mut reset: RoundReset,
) {
    if !net_mode.is_network() {
        reset.game_start(&settings);
    }
}

/// Handle queued missile impacts: scoring, marking players as shot, etc.
#[allow(clippy::too_many_arguments)]
pub fn handle_missile_impact(
    mut impact_queue: ResMut<MissileImpactQueue>,
    mut players: Query<&mut Player>,
    missile_q: Query<&MissileMarker>,
    mut turn: ResMut<TurnState>,
    mut spawn_queue: ResMut<ParticleSpawnQueue>,
    settings: Res<GameSettings>,
    mut round_result: ResMut<RoundResult>,
    net: Res<gnils_net::NetState>,
) {
    for impact in std::mem::take(&mut impact_queue.impacts) {
        // A hit throws off a burst of debris, if debris is switched on.
        let mut explode = |on_screen_only: bool| {
            let visible =
                !on_screen_only || is_on_screen((impact.pos.x as f64, impact.pos.y as f64));
            if settings.particles_enabled && visible {
                spawn_queue.requests.push(ParticleSpawnRequest {
                    pos: impact.pos,
                    count: N_PARTICLES_10,
                    size: 10,
                });
            }
        };

        match impact.hit_type {
            HitType::Planet => {
                info!("Missile hit planet at ({}, {})", impact.pos.x, impact.pos.y);
                explode(true);
                end_shot(&mut turn);
            }
            HitType::Blackhole => {
                info!("Missile absorbed by blackhole");
                end_shot(&mut turn);
            }
            HitType::Ship(hit_id) => {
                explode(false);

                let last = turn.last_player;
                let killed_self = last == hit_id;
                let power_penalty = missile_q.iter().next().map_or(0, |m| m.power_penalty);

                let mut shooter_attempts = 0u32;
                for mut player in players.iter_mut() {
                    if player.id == hit_id {
                        player.shot = true;
                    }
                    if player.id == last {
                        shooter_attempts = player.attempts;
                    }
                }

                let (total_delta, quick_bonus, pen) =
                    compute_shot_score(killed_self, power_penalty, shooter_attempts);

                let scored_by = if killed_self { hit_id } else { last };
                for mut player in players.iter_mut() {
                    if player.id == scored_by {
                        player.score += total_delta;
                    }
                }

                // Roster names in network games; "Player n" in hotseat play.
                let name_of = |id: u8| {
                    net.player_name(id)
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("Player {id}"))
                };
                *round_result = RoundResult {
                    hit_player: hit_id,
                    self_hit: killed_self,
                    // Shown as the original showed it: the self-hit is a
                    // positive figure that the message says is deducted.
                    hit_score: if killed_self { SELF_HIT } else { HIT_SCORE },
                    quick_bonus,
                    power_penalty: pen,
                    total_score: total_delta,
                    message: if killed_self {
                        format!("{} killed self", name_of(last))
                    } else {
                        format!("{} killed {}", name_of(last), name_of(hit_id))
                    },
                };

                info!("Ship {} hit! Round over.", hit_id);
                turn.round_over = true;
                turn.firing = false;

                // Only the final round gets the zoom text.
                if settings.max_rounds > 0 && turn.round >= settings.max_rounds {
                    turn.game_over = true;
                    turn.show_round = 100.0;
                }
            }
        }
    }
}

fn end_shot(turn: &mut TurnState) {
    let next = turn.other_player();
    info!("End shot: next player = {}", next);
    turn.current_player = next;
    turn.firing = false;
}

/// Round setup: increment round counter, randomize player positions, and
/// transition to aiming.
///
/// In network mode the player-Y draws come from the shared `NetSeed` (derived
/// `base ^ round`) so both peers place ships identically.
pub fn round_setup(
    mut turn: ResMut<TurnState>,
    mut next_state: ResMut<NextState<GamePhase>>,
    mut settings: ResMut<GameSettings>,
    mut players: Query<(&mut Player, &mut Transform)>,
    net_seed: Option<Res<NetSeed>>,
    net_mode: Res<NetworkMode>,
) {
    turn.round += 1;

    let mut rng = layout_rng(&net_mode, net_seed.as_deref(), turn.round);

    // Random mode (network only): each round randomizes game style settings.
    // Both peers derive the same values from the shared seed.
    let randomize = net_mode.is_network() && settings.random;
    if randomize {
        settings.bounce = rng.random_bool(0.5);
        settings.fixed_power = rng.random_bool(0.5);
        settings.invisible = rng.random_bool(0.5);
    }

    // Randomize player Y positions each round
    for (mut player, mut transform) in players.iter_mut() {
        let y = rng.random_range(PLAYER_Y_MIN..=PLAYER_Y_MAX);
        let x = if player.id == 1 { PLAYER1_X } else { PLAYER2_X };
        transform.translation.x = x as f32;
        transform.translation.y = y as f32;
        if randomize {
            player.power = if settings.fixed_power {
                FIXED_POWER_VALUE
            } else {
                100.0
            };
        }
    }

    turn.show_planets = show_planets_for(&settings);

    next_state.set(GamePhase::Aiming);
}
