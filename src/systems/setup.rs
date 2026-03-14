use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::components::*;
use crate::constants::*;
use crate::resources::*;
use crate::systems::player::pygame_to_bevy;

pub fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

pub fn load_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // Ship sprite strip: 320x33, 8 frames of 40x33
    let ship_layout = TextureAtlasLayout::from_grid(
        UVec2::new(SHIP_FRAME_WIDTH, SHIP_FRAME_HEIGHT),
        8,
        1,
        None,
        None,
    );
    let ship_atlas_layout = atlas_layouts.add(ship_layout);

    let assets = GameAssets {
        font: asset_server.load("FreeSansBold.ttf"),
        backdrop: asset_server.load("backdrop.png"),
        red_ship: asset_server.load("red_ship.png"),
        blue_ship: asset_server.load("blue_ship.png"),
        ship_atlas_layout,
        shot: asset_server.load("shot.png"),
        explosion: asset_server.load("explosion.png"),
        explosion_10: asset_server.load("explosion-10.png"),
        explosion_5: asset_server.load("explosion-5.png"),
        planets: [
            asset_server.load("planet_1.png"),
            asset_server.load("planet_2.png"),
            asset_server.load("planet_3.png"),
            asset_server.load("planet_4.png"),
            asset_server.load("planet_5.png"),
            asset_server.load("planet_6.png"),
            asset_server.load("planet_7.png"),
            asset_server.load("planet_8.png"),
        ],
    };
    commands.insert_resource(assets);
}

pub fn setup_trail_canvas(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = Extent3d {
        width: WINDOW_WIDTH as u32,
        height: WINDOW_HEIGHT as u32,
        depth_or_array_layers: 1,
    };

    let image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    );

    let handle = images.add(image);

    commands.spawn((
        Sprite {
            image: handle.clone(),
            color: Color::srgba(1.0, 1.0, 1.0, 125.0 / 255.0),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 3.0),
        TrailSprite,
    ));

    commands.insert_resource(TrailCanvas {
        image_handle: handle,
    });
}

pub fn setup_background(mut commands: Commands, assets: Res<GameAssets>) {
    commands.spawn((
        Sprite::from_image(assets.backdrop.clone()),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}

pub fn setup_players(mut commands: Commands, assets: Res<GameAssets>) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let y1 = rng.gen_range(PLAYER_Y_MIN..=PLAYER_Y_MAX);
    let y2 = rng.gen_range(PLAYER_Y_MIN..=PLAYER_Y_MAX);

    let p1_bevy = pygame_to_bevy(40.0, y1);

    commands.spawn((
        Sprite::from_atlas_image(
            assets.red_ship.clone(),
            TextureAtlas {
                layout: assets.ship_atlas_layout.clone(),
                index: 0,
            },
        ),
        Transform::from_xyz(p1_bevy.x, p1_bevy.y, 4.0),
        Player {
            id: 1,
            angle: 90.0,
            rel_rot: 0.01,
            power: 100.0,
            score: 0,
            attempts: 0,
            shot: false,
            color_rgb: PLAYER1_COLOR,
            gun_offset: 22.0,
            explosion_frame: 0,
        },
    ));

    // Blend layer sprite for player 1 (same position, slightly higher Z, different atlas frame)
    commands.spawn((
        Sprite::from_atlas_image(
            assets.red_ship.clone(),
            TextureAtlas {
                layout: assets.ship_atlas_layout.clone(),
                index: 0,
            },
        ),
        Transform::from_xyz(p1_bevy.x, p1_bevy.y, 4.05),
        crate::components::ShipBlendSprite { player_id: 1 },
    ));

    let p2_bevy = pygame_to_bevy(760.0, y2);

    commands.spawn((
        Sprite::from_atlas_image(
            assets.blue_ship.clone(),
            TextureAtlas {
                layout: assets.ship_atlas_layout.clone(),
                index: 0,
            },
        ),
        Transform::from_xyz(p2_bevy.x, p2_bevy.y, 4.0),
        Player {
            id: 2,
            angle: 270.0,
            rel_rot: 0.01,
            power: 100.0,
            score: 0,
            attempts: 0,
            shot: false,
            color_rgb: PLAYER2_COLOR,
            gun_offset: 23.0,
            explosion_frame: 0,
        },
    ));

    // Blend layer sprite for player 2
    commands.spawn((
        Sprite::from_atlas_image(
            assets.blue_ship.clone(),
            TextureAtlas {
                layout: assets.ship_atlas_layout.clone(),
                index: 0,
            },
        ),
        Transform::from_xyz(p2_bevy.x, p2_bevy.y, 4.05),
        crate::components::ShipBlendSprite { player_id: 2 },
    ));
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
        crate::components::ZoomDimSprite,
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

pub fn setup_ui(mut commands: Commands, assets: Res<GameAssets>) {
    commands.spawn((
        Text::new("Player 1  --  0"),
        TextFont {
            font: assets.font.clone(),
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(
            PLAYER1_COLOR.0 as f32 / 255.0,
            PLAYER1_COLOR.1 as f32 / 255.0,
            PLAYER1_COLOR.2 as f32 / 255.0,
        )),
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
        TextFont {
            font: assets.font.clone(),
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(
            PLAYER2_COLOR.0 as f32 / 255.0,
            PLAYER2_COLOR.1 as f32 / 255.0,
            PLAYER2_COLOR.2 as f32 / 255.0,
        )),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(5.0),
            right: Val::Px(6.0),
            ..default()
        },
        UiScoreP2,
    ));

    commands.spawn((
        Text::new("Angle: 90.00  Power: 100.0"),
        TextFont {
            font: assets.font.clone(),
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(5.0),
            left: Val::Px(290.0),
            ..default()
        },
        UiAnglePower,
    ));

    commands.spawn((
        Text::new("Round 1"),
        TextFont {
            font: assets.font.clone(),
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(6.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        UiRoundInfo,
    ));

    commands.spawn((
        Text::new(""),
        TextFont {
            font: assets.font.clone(),
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(5.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Visibility::Hidden,
        UiMissileStatus,
    ));

    // Round overlay text (centered, zooming "Round N" text, z=10)
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
            ZIndex(10),
            UiRoundOverlay,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Round 1"),
                TextFont {
                    font: assets.font.clone(),
                    font_size: 48.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });

    // Settings menu overlay (full-screen, hidden by default)
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(0.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Visibility::Hidden,
        ZIndex(20),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        crate::components::UiMenuOverlay,
    )).with_children(|parent| {
        parent.spawn((
            Node {
                padding: UiRect::axes(Val::Px(50.0), Val::Px(30.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.1, 0.95)),
            BorderColor::all(Color::srgb(0.0, 0.0, 0.8)),
        )).with_children(|box_parent| {
            box_parent.spawn((
                Text::new(""),
                TextFont {
                    font: assets.font.clone(),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                crate::components::UiMenuText,
            ));
        });
    });

    // End round message container (centered box with dark background)
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
            ZIndex(10),
            UiEndRoundMsg,
        ))
        .with_children(|parent| {
            // Dark box with border
            parent
                .spawn((
                    Node {
                        padding: UiRect::axes(Val::Px(50.0), Val::Px(35.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 175.0 / 255.0)),
                    BorderColor::all(Color::srgb(
                        150.0 / 255.0,
                        150.0 / 255.0,
                        150.0 / 255.0,
                    )),
                ))
                .with_children(|box_parent| {
                    // Text inside the box
                    box_parent.spawn((
                        Text::new(""),
                        TextFont {
                            font: assets.font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        UiDimOverlay, // reuse as marker for the text node
                    ));
                });
        });
}
