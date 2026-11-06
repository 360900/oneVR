//! oneVR: where things go by default. The proportions are the WiVRn lobby's, from
//! `wivrn/client/constants.h` and `wivrn/client/scenes/lobby.cpp`, scaled about the head to where
//! the eyes focus: everything looks the same size, at the same angles, as in the lobby.

use glam::{Affine3A, Quat, Vec3, Vec3A, vec3};

/// The lobby's main window is this far from the head
const LOBBY_DISTANCE: f32 = 0.5;
/// From the head to the seat, along the view on the horizontal plane. Headset lenses focus at a
/// fixed distance, about 1.3 m on the Quest 2. Near it the eyes aim where they focus, which rests
/// them over long sessions.
const SEAT_DISTANCE: f32 = 1.3;
/// What the lobby's sizes and offsets are multiplied by
pub const LAYOUT_SCALE: f32 = SEAT_DISTANCE / LOBBY_DISTANCE;
/// How far the seat is below the eyes
const SEAT_DROP: f32 = 0.1 * LAYOUT_SCALE;
/// Tilted back, to face eyes that look slightly down
const SEAT_PITCH_DEG: f32 = -12.0;

/// The dashboard fills the seat, in the lobby's main window's view
pub const DASHBOARD_WIDTH: f32 = 0.6 * LAYOUT_SCALE;

/// Screens and windows are behind the dashboard, so it opens in front of them
const PANEL_BEHIND_SEAT: f32 = 0.1 * LAYOUT_SCALE;
/// Fills the same view as the dashboard does, from further away
pub const PANEL_WIDTH: f32 = DASHBOARD_WIDTH * (SEAT_DISTANCE + PANEL_BEHIND_SEAT) / SEAT_DISTANCE;
/// As wide as the dashboard with WayVR's default layout of 23 keys per row, as the lobby's is
pub const KEYBOARD_KEY_WIDTH: f32 = DASHBOARD_WIDTH / 23.0;
/// Relative to the seat, as the lobby keeps its keyboard to its main window
pub const KEYBOARD_POSITION: Vec3 = vec3(0.0, -0.3 * LAYOUT_SCALE, 0.1 * LAYOUT_SCALE);
pub const KEYBOARD_PITCH_RAD: f32 = -0.6;
/// How far the anchor is, compared to the 1 m it used to be
pub const OLD_ANCHOR_DISTANCE_RATIO: f32 = SEAT_DISTANCE;
/// What a panel sized for the old default of 1.5 m from the head needs to fill the same view
pub const OLD_DISTANCE_TO_PANEL: f32 = (SEAT_DISTANCE + PANEL_BEHIND_SEAT) / 1.5;

/// How far apart the panels of a set are around the head
const PANEL_GAP_DEG: f32 = 5.0;
const PANELS_PER_ROW: usize = 3;

/// A head moved this far from where the layout was placed has left it
const LEFT_LAYOUT_METERS: f32 = 1.5;

/// The seat, relative to [`level_head`]
#[must_use]
pub fn seat_local() -> Affine3A {
    Affine3A::from_rotation_translation(
        Quat::from_rotation_x(SEAT_PITCH_DEG.to_radians()),
        vec3(0.0, -SEAT_DROP, -SEAT_DISTANCE),
    )
}

/// The direction the head faces on the horizontal plane. Looking straight up or down there is
/// none, where the top of the head points the same way.
fn level_forward(hmd: &Affine3A) -> Vec3A {
    let level = |v: Vec3A| Vec3A::new(v.x, 0.0, v.z).normalize_or_zero();
    let forward = level(-hmd.z_axis);
    if forward == Vec3A::ZERO || hmd.z_axis.normalize().y.abs() > 0.95 {
        let up = level(hmd.y_axis * hmd.z_axis.y.signum());
        if up != Vec3A::ZERO {
            return up;
        }
    }
    if forward == Vec3A::ZERO {
        Vec3A::NEG_Z
    } else {
        forward
    }
}

/// The head without its pitch and roll: looking down or leaning doesn't tip the layout
#[must_use]
pub fn level_head(hmd: &Affine3A) -> Affine3A {
    let forward = level_forward(hmd);
    let yaw = (-forward.x).atan2(-forward.z);
    Affine3A::from_rotation_translation(Quat::from_rotation_y(yaw), hmd.translation.into())
}

/// Where the lobby would open for this head. The anchor goes here, so a default transform is
/// relative to the lobby's main window.
#[must_use]
pub fn seat(hmd: &Affine3A) -> Affine3A {
    level_head(hmd) * seat_local()
}

/// A screen or window that nothing else was decided for
#[must_use]
pub fn panel_transform(scale: f32, rotation: Quat) -> Affine3A {
    Affine3A::from_scale_rotation_translation(
        Vec3::ONE * PANEL_WIDTH * scale,
        rotation,
        vec3(0.0, 0.0, -PANEL_BEHIND_SEAT),
    )
}

/// Whether the head left where it was when the layout was placed, `placed` being that pose
#[must_use]
pub fn head_left(hmd: &Affine3A, placed: &Affine3A) -> bool {
    hmd.translation.distance(placed.translation) >= LEFT_LAYOUT_METERS
        || level_forward(hmd).dot(level_forward(placed)) <= 0.0
}

/// Spots for `count` panels side by side around the head, each still facing it, in rows of three
/// going up. They apply to a panel's default transform, from the seat; `width` is the widest's.
#[must_use]
pub fn arc_slots(count: usize, width: f32) -> Vec<Affine3A> {
    let reach = SEAT_DISTANCE + PANEL_BEHIND_SEAT;
    let yaw_step = 2.0 * (width / 2.0 / reach).atan() + PANEL_GAP_DEG.to_radians();
    // as tall as a 16:9 panel
    let pitch_step = 2.0 * (width * 9.0 / 32.0 / reach).atan() + PANEL_GAP_DEG.to_radians();
    let seat = seat_local();
    let from_seat = seat.inverse();

    let mut slots = Vec::with_capacity(count);
    for (row, chunk) in (0..count)
        .collect::<Vec<_>>()
        .chunks(PANELS_PER_ROW)
        .enumerate()
    {
        for col in 0..chunk.len() {
            let along = col as f32 - (chunk.len() - 1) as f32 / 2.0;
            // the first panel is on the left
            let around_head = Affine3A::from_rotation_y(-along * yaw_step)
                * Affine3A::from_rotation_x(row as f32 * pitch_step);
            slots.push(from_seat * around_head * seat);
        }
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(yaw: f32, pitch: f32, roll: f32) -> Affine3A {
        Affine3A::from_rotation_translation(
            Quat::from_euler(glam::EulerRot::YXZ, yaw, pitch, roll),
            vec3(1.0, 1.7, -2.0),
        )
    }

    #[test]
    fn lobby_numbers() {
        // the lobby's 0.5 m ahead and 0.1 m down, scaled to 1.3 m ahead, tilted 12 degrees
        let s = seat(&head(0.0, 0.0, 0.0));
        let offset = s.translation - Vec3A::new(1.0, 1.7, -2.0);
        assert!(offset.abs_diff_eq(Vec3A::new(0.0, -0.26, -1.3), 1e-5));
        assert!((s.z_axis.y - 12f32.to_radians().sin()).abs() < 1e-5);
    }

    #[test]
    fn head_pitch_and_roll_do_not_tip_the_seat() {
        let level = seat(&head(0.7, 0.0, 0.0));
        for (pitch, roll) in [(-1.0, 0.0), (1.0, 0.0), (0.0, 0.8), (-0.6, -0.5)] {
            let tipped = seat(&head(0.7, pitch, roll));
            assert!(tipped.abs_diff_eq(level, 1e-4), "{pitch} {roll}");
        }
    }

    #[test]
    fn looking_straight_down_keeps_a_direction() {
        let hmd = head(0.7, -std::f32::consts::FRAC_PI_2, 0.0);
        let level = seat(&head(0.7, 0.0, 0.0));
        assert!(seat(&hmd).abs_diff_eq(level, 1e-3));
    }

    #[test]
    fn keyboard_is_under_the_dashboard() {
        let dashboard_bottom = -DASHBOARD_WIDTH * 9.0 / 16.0 / 2.0;
        let keyboard_height = DASHBOARD_WIDTH * 6.0 / 23.0;
        let keyboard_top = KEYBOARD_POSITION.y + keyboard_height / 2.0 * KEYBOARD_PITCH_RAD.cos();
        assert!(keyboard_top < dashboard_bottom);
    }

    #[test]
    fn looks_the_same_size_as_the_lobby() {
        let lobby = (0.3f32 / 0.5).atan();
        let dashboard = (DASHBOARD_WIDTH / 2.0 / SEAT_DISTANCE).atan();
        assert!((lobby - dashboard).abs() < 1e-5);
        assert!((DASHBOARD_WIDTH - 1.56).abs() < 1e-4);
    }

    #[test]
    fn panel_fills_the_view_of_the_dashboard() {
        let dashboard = (DASHBOARD_WIDTH / 2.0 / SEAT_DISTANCE).atan();
        let panel = (PANEL_WIDTH / 2.0 / (SEAT_DISTANCE + PANEL_BEHIND_SEAT)).atan();
        assert!((dashboard - panel).abs() < 1e-5);
    }

    #[test]
    fn one_panel_stays_centered() {
        let slots = arc_slots(1, PANEL_WIDTH);
        assert!(slots[0].abs_diff_eq(Affine3A::IDENTITY, 1e-5));
    }

    #[test]
    fn panels_do_not_overlap_and_face_the_head() {
        for count in 2..=7 {
            let world: Vec<_> = arc_slots(count, PANEL_WIDTH)
                .into_iter()
                .map(|slot| seat_local() * slot * panel_transform(1.0, Quat::IDENTITY))
                .collect();
            for (i, a) in world.iter().enumerate() {
                // same distance from the head, tilted the way the first one is
                assert!((a.translation.length() - world[0].translation.length()).abs() < 1e-4);
                let to_head = -a.translation.normalize();
                assert!(a.z_axis.normalize().dot(to_head) > 0.99, "{count} {i}");
                for b in &world[i + 1..] {
                    let angle = a
                        .translation
                        .normalize()
                        .dot(b.translation.normalize())
                        .clamp(-1.0, 1.0)
                        .acos();
                    // at least as far apart as the shorter side plus the gap
                    assert!(angle > 40f32.to_radians(), "{count} {i} {angle}");
                }
            }
        }
    }

    #[test]
    fn first_panel_is_on_the_left() {
        let slots = arc_slots(2, PANEL_WIDTH);
        let x = |slot: &Affine3A| {
            (seat_local() * *slot * panel_transform(1.0, Quat::IDENTITY))
                .translation
                .x
        };
        assert!(x(&slots[0]) < x(&slots[1]));
    }

    #[test]
    fn turning_or_walking_leaves_the_layout() {
        let placed = head(0.0, 0.0, 0.0);
        assert!(!head_left(&head(1.0, 0.8, 0.3), &placed));
        assert!(head_left(&head(2.0, 0.0, 0.0), &placed));
        let mut walked = placed;
        walked.translation.x += 2.0;
        assert!(head_left(&walked, &placed));
    }
}
