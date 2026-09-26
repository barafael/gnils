use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use gnils_protocol::{GUN_OFFSET_P1, GUN_OFFSET_P2};

use crate::components::*;
use crate::constants::*;
use crate::resources::*;

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: WINDOW_WIDTH,
                min_height: WINDOW_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
}

/// A blank CPU-side RGBA canvas the game draws into each frame.
pub(crate) fn blank_image(width: u32, height: u32) -> Image {
    Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    )
}

pub fn load_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.insert_resource(GameAssets {
        font: asset_server.load("FreeSansBold.ttf"),
        backdrop: asset_server.load("backdrop.png"),
        red_ship: asset_server.load("red_ship.png"),
        blue_ship: asset_server.load("blue_ship.png"),
        shot: asset_server.load("shot.png"),
        explosion: asset_server.load("explosion.png"),
        explosion_10: asset_server.load("explosion-10.png"),
        explosion_5: asset_server.load("explosion-5.png"),
        planets: std::array::from_fn(|i| asset_server.load(format!("planet_{}.png", i + 1))),
    });

    // Pre-allocate blended ship images (one per player, updated each frame).
    commands.insert_resource(BlendedShipImages {
        handles: std::array::from_fn(|_| {
            images.add(blank_image(SHIP_FRAME_WIDTH, SHIP_FRAME_HEIGHT))
        }),
    });
}

pub fn setup_trail_canvas(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // Sized for the 4:3 playfield to begin with; `resize_trail_canvas`
    // grows it to whatever the window actually shows.
    let size = UVec2::new(WINDOW_WIDTH as u32, WINDOW_HEIGHT as u32);
    let handle = images.add(blank_image(size.x, size.y));

    commands.spawn((
        Sprite {
            image: handle.clone(),
            color: Color::srgba(1.0, 1.0, 1.0, 125.0 / 255.0),
            custom_size: Some(size.as_vec2()),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 3.0),
        TrailSprite,
    ));

    commands.insert_resource(TrailCanvas {
        image_handle: handle,
        size,
    });
}

pub fn setup_background(mut commands: Commands, assets: Res<GameAssets>) {
    commands.spawn((
        Sprite::from_image(assets.backdrop.clone()),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Backdrop,
    ));
}

pub fn setup_players(mut commands: Commands, blended: Res<BlendedShipImages>) {
    use rand::RngExt;
    let mut rng = rand::rng();

    for (id, x, color_rgb, gun_offset) in [
        (1u8, PLAYER1_X, PLAYER1_COLOR, GUN_OFFSET_P1),
        (2, PLAYER2_X, PLAYER2_COLOR, GUN_OFFSET_P2),
    ] {
        let y = rng.random_range(PLAYER_Y_MIN..=PLAYER_Y_MAX);
        commands.spawn((
            Sprite::from_image(blended.handles[(id - 1) as usize].clone()),
            Transform::from_xyz(x as f32, y as f32, 4.0),
            Player {
                id,
                // Radians CCW from east: 0 = east, π = west.
                angle: initial_angle(id),
                rel_rot: 0.0,
                power: 100.0,
                score: 0,
                attempts: 0,
                shot: false,
                color_rgb,
                gun_offset,
                explosion_progress: 0.0,
            },
        ));
    }
}

/// Spawn the full-screen dim sprite used behind the zoom minimap.
pub fn setup_zoom_dim(mut commands: Commands) {
    commands.spawn((
        Sprite {
            color: Color::srgba(0.0, 0.0, 0.0, 175.0 / 255.0),
            custom_size: Some(Vec2::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 18.0),
        Visibility::Hidden,
        ZoomDimSprite,
    ));
}

pub fn setup_missile(mut commands: Commands, assets: Res<GameAssets>) {
    commands.spawn((
        Sprite::from_image(assets.shot.clone()),
        Transform::from_xyz(0.0, 0.0, 6.0),
        Visibility::Hidden,
        MissileMarker {
            trail_color: PLAYER1_COLOR,
            power_penalty: 0,
            active: false,
        },
        GravityBody {
            pos: (0.0, 0.0),
            velocity: (0.0, 0.0),
            last_pos: (0.0, 0.0),
            flight: 0,
        },
    ));
}

/// Every HUD and overlay line is set in the same size.
const HUD_FONT_SIZE: f32 = 14.0;

fn hud_font(font: &Handle<Font>, size: f32) -> TextFont {
    TextFont {
        font: font.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// A full-width row pinned to one edge of the screen, its contents centered.
fn hud_row(top: Val, bottom: Val) -> Node {
    Node {
        position_type: PositionType::Absolute,
        top,
        bottom,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        justify_content: JustifyContent::Center,
        ..default()
    }
}

/// A node covering the whole screen with its contents centered — the base of
/// the round, menu and end-of-round overlays.
fn overlay() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: Val::Px(0.0),
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        bottom: Val::Px(0.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

/// A HUD text line centered across the window: a full-width container node
/// with the marked Text as its only child. Systems address the text through
/// the marker; visibility is toggled on the child.
fn spawn_hud_text(
    commands: &mut Commands,
    font: &Handle<Font>,
    container: Node,
    marker: impl Component,
    initial: Visibility,
) {
    commands.spawn(container).with_children(|parent| {
        parent.spawn((
            Text::new(""),
            hud_font(font, HUD_FONT_SIZE),
            TextColor(Color::WHITE),
            initial,
            marker,
        ));
    });
}

pub fn setup_ui(mut commands: Commands, assets: Res<GameAssets>) {
    let font = &assets.font;

    // Scores, in each player's own colour, in their own top corner.
    commands.spawn((
        Text::new("Player 1  --  0"),
        hud_font(font, HUD_FONT_SIZE),
        TextColor(rgb(PLAYER1_COLOR)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(5.0),
            left: Val::Px(5.0),
            ..default()
        },
        UiScoreP1,
    ));
    commands.spawn((
        Text::new("0  --  Player 2"),
        hud_font(font, HUD_FONT_SIZE),
        TextColor(rgb(PLAYER2_COLOR)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(5.0),
            right: Val::Px(6.0),
            ..default()
        },
        UiScoreP2,
    ));

    // Centered HUD lines. The text must be a child of a stretching
    // container: a bare Text node keeps its content size, so
    // justify_content on it would never center anything.
    let top = |px| hud_row(Val::Px(px), Val::Auto);
    spawn_hud_text(
        &mut commands,
        font,
        top(5.0),
        UiAnglePower,
        Visibility::Visible,
    );
    spawn_hud_text(
        &mut commands,
        font,
        hud_row(Val::Auto, Val::Px(6.0)),
        UiRoundInfo,
        Visibility::Visible,
    );
    spawn_hud_text(
        &mut commands,
        font,
        top(5.0),
        UiMissileStatus,
        Visibility::Hidden,
    );
    // Turn banner (network games only): whose turn it is while aiming.
    spawn_hud_text(
        &mut commands,
        font,
        top(25.0),
        UiTurnBanner,
        Visibility::Hidden,
    );

    // Round overlay text (centered, zooming "Round N" text).
    commands
        .spawn((overlay(), Visibility::Hidden, ZIndex(10), UiRoundOverlay))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Round 1"),
                hud_font(font, 48.0),
                TextColor(Color::WHITE),
            ));
        });

    // Settings menu overlay (full-screen, hidden by default).
    commands
        .spawn((
            overlay(),
            Visibility::Hidden,
            ZIndex(20),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            UiMenuOverlay,
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(50.0), Val::Px(30.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.1, 0.95)),
                BorderColor::all(Color::srgb(0.0, 0.0, 0.8)),
                UiMenuColumn,
            ));
        });

    // End-of-round message: a bordered dark box around a single text node.
    commands
        .spawn((overlay(), Visibility::Hidden, ZIndex(10), UiEndRoundMsg))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(50.0), Val::Px(35.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 175.0 / 255.0)),
                    BorderColor::all(rgb((150, 150, 150))),
                ))
                .with_children(|box_parent| {
                    box_parent.spawn((
                        Text::new(""),
                        hud_font(font, HUD_FONT_SIZE),
                        TextColor(Color::WHITE),
                        UiDimOverlay, // reuse as marker for the text node
                    ));
                });
        });
}
