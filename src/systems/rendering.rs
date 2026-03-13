use bevy::prelude::*;

use crate::components::*;
use crate::resources::*;

/// Update bounce border animation.
pub fn update_bounce_animation(mut bounce: ResMut<BounceAnimation>) {
    bounce.count += bounce.inc;
    if bounce.count > 255.0 || bounce.count < 125.0 {
        bounce.inc *= -1.0;
        bounce.count += 2.0 * bounce.inc;
    }
}

/// Draw bounce border using gizmos.
pub fn draw_bounce_border(
    mut gizmos: Gizmos,
    settings: Res<crate::resources::GameSettings>,
    bounce: Res<BounceAnimation>,
) {
    if !settings.bounce {
        return;
    }

    let r = bounce.count / 255.0;
    let color = Color::srgb(r, 0.0, 0.0);

    // Draw rectangle border (800x600 in bevy coords = centered at 0,0)
    let half_w = 400.0;
    let half_h = 300.0;

    gizmos.line_2d(
        Vec2::new(-half_w, -half_h),
        Vec2::new(half_w, -half_h),
        color,
    );
    gizmos.line_2d(Vec2::new(half_w, -half_h), Vec2::new(half_w, half_h), color);
    gizmos.line_2d(Vec2::new(half_w, half_h), Vec2::new(-half_w, half_h), color);
    gizmos.line_2d(
        Vec2::new(-half_w, half_h),
        Vec2::new(-half_w, -half_h),
        color,
    );
}

/// Hide/show angle-power UI based on game state.
pub fn update_ui_visibility(
    turn: Res<TurnState>,
    mut angle_power: Query<&mut Visibility, (With<UiAnglePower>, Without<UiMissileStatus>)>,
    mut missile_status: Query<&mut Visibility, (With<UiMissileStatus>, Without<UiAnglePower>)>,
) {
    for mut vis in angle_power.iter_mut() {
        if turn.firing || turn.round_over {
            *vis = Visibility::Hidden;
        } else {
            *vis = Visibility::Visible;
        }
    }

    for mut vis in missile_status.iter_mut() {
        if turn.firing {
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}
