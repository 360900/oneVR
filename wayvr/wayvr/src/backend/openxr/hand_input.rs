//! oneVR: hand tracking input that the runtime's values alone get wrong.

use std::f32::consts::TAU;

use glam::{Affine3A, Quat, Vec3};
use openxr as xr;

/// A fist has to last this long, which drops the runtime's spikes
const GRASP_HOLD_SECS: f32 = 0.12;
/// [`index_bend`] at or below this may start a grab, about 80 degrees
const INDEX_CURLED: f32 = 0.2;
/// [`index_bend`] above this ends a grab, about 50 degrees
const INDEX_EXTENDED: f32 = 0.6;

/// [`PoseFilter`]: the cutoff of a still hand, in Hz. Lower hides more jitter and lags more.
const POSE_MIN_CUTOFF: f32 = 2.0;
/// [`PoseFilter`]: how fast the cutoff rises, in Hz per m/s. Higher lags less in a fast move.
const POSITION_BETA: f32 = 10.0;
/// [`PoseFilter`]: as [`POSITION_BETA`], in Hz per rad/s
const ROTATION_BETA: f32 = 2.0;
/// [`PoseFilter`]: smooths the measured speed, in Hz
const SPEED_CUTOFF: f32 = 1.0;

/// The share of the way to a new sample that a low-pass filter with this cutoff takes
fn low_pass(cutoff: f32, secs: f32) -> f32 {
    let step = TAU * cutoff * secs;
    step / (1.0 + step)
}

/// A One Euro filter for the pointer of a tracked hand: strong while the hand is nearly still, where
/// jitter shows on a far panel, and weak while it moves, where lag would show.
#[derive(Default)]
pub(super) struct PoseFilter {
    last: Option<(Vec3, Quat)>,
    position_speed: f32,
    rotation_speed: f32,
}

impl PoseFilter {
    pub(super) fn update(&mut self, pose: Affine3A, secs: f32) -> Affine3A {
        let (position, rotation) = (Vec3::from(pose.translation), Quat::from_affine3(&pose));
        let Some((last_position, last_rotation)) = self.last else {
            self.last = Some((position, rotation));
            return pose;
        };

        let speed_step = low_pass(SPEED_CUTOFF, secs);
        self.position_speed +=
            speed_step * (position.distance(last_position) / secs - self.position_speed);
        self.rotation_speed +=
            speed_step * (rotation.angle_between(last_rotation) / secs - self.rotation_speed);

        let position_step = low_pass(POSE_MIN_CUTOFF + POSITION_BETA * self.position_speed, secs);
        let rotation_step = low_pass(POSE_MIN_CUTOFF + ROTATION_BETA * self.rotation_speed, secs);
        let position = last_position.lerp(position, position_step);
        let rotation = last_rotation.slerp(rotation, rotation_step);
        self.last = Some((position, rotation));
        Affine3A::from_rotation_translation(rotation, position)
    }

    /// The next pose is taken as it is, as after the hand was lost
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// How straight the index finger is: the cosine of the angle between its first and last segments,
/// 1 when straight and below 0 in a fist. None without valid joints.
pub(super) fn index_bend(joints: &xr::HandJointLocations) -> Option<f32> {
    let position = |joint: xr::HandJoint| {
        let location = &joints[joint];
        location
            .location_flags
            .contains(xr::SpaceLocationFlags::POSITION_VALID)
            .then(|| {
                let p = location.pose.position;
                Vec3::new(p.x, p.y, p.z)
            })
    };
    let first =
        position(xr::HandJoint::INDEX_INTERMEDIATE)? - position(xr::HandJoint::INDEX_PROXIMAL)?;
    let last = position(xr::HandJoint::INDEX_TIP)? - position(xr::HandJoint::INDEX_DISTAL)?;
    Some(first.try_normalize()?.dot(last.try_normalize()?))
}

/// Decides whether a hand's `grasp_ext` value counts. Runtimes can report a high value for a hand
/// that points, whose other fingers are curled, and a grab would start on the first such frame.
pub(super) struct GraspGate {
    /// The grab's [release, press]
    thresholds: [f32; 2],
    held_secs: f32,
    open: bool,
}

impl GraspGate {
    pub(super) const fn new(thresholds: [f32; 2]) -> Self {
        Self {
            thresholds,
            held_secs: 0.0,
            open: false,
        }
    }

    /// `bend` is None when the joints say nothing, then the fist only has to last: lost joints
    /// must not make grabbing impossible.
    pub(super) fn update(&mut self, value: f32, bend: Option<f32>, dt: f32) -> bool {
        if self.open {
            let extended = bend.is_some_and(|bend| bend > INDEX_EXTENDED);
            self.open = value >= self.thresholds[0] && !extended;
            self.held_secs = 0.0;
            return self.open;
        }
        let curled = bend.is_none_or(|bend| bend <= INDEX_CURLED);
        if value >= self.thresholds[1] && curled {
            self.held_secs += dt;
            self.open = self.held_secs >= GRASP_HOLD_SECS;
        } else {
            self.held_secs = 0.0;
        }
        self.open
    }

    pub(super) const fn reset(&mut self) {
        self.held_secs = 0.0;
        self.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 72.0;
    const FIST: Option<f32> = Some(-0.9);
    const POINTING: Option<f32> = Some(0.95);

    fn gate() -> GraspGate {
        GraspGate::new([0.7, 0.9])
    }

    fn run(gate: &mut GraspGate, value: f32, bend: Option<f32>, secs: f32) -> bool {
        let mut open = false;
        for _ in 0..(secs / DT).ceil() as usize {
            open = gate.update(value, bend, DT);
        }
        open
    }

    fn pose(x: f32) -> Affine3A {
        Affine3A::from_translation(Vec3::new(x, 0.0, 0.0))
    }

    #[test]
    fn the_first_pose_passes() {
        let mut filter = PoseFilter::default();
        assert_eq!(filter.update(pose(1.0), DT).translation.x, 1.0);
    }

    #[test]
    fn jitter_shrinks_while_still() {
        let mut filter = PoseFilter::default();
        filter.update(pose(0.0), DT);
        let mut furthest = 0.0f32;
        for i in 0..(2.0 / DT) as usize {
            let jitter = if i % 2 == 0 { 0.002 } else { -0.002 };
            furthest = furthest.max(filter.update(pose(jitter), DT).translation.x.abs());
        }
        assert!(furthest < 0.001, "{furthest}");
    }

    #[test]
    fn a_fast_move_follows_closely() {
        let mut filter = PoseFilter::default();
        filter.update(pose(0.0), DT);
        // 2 m/s for a quarter of a second
        let frames = (0.25 / DT) as usize;
        let mut last = 0.0;
        for i in 1..=frames {
            last = filter.update(pose(i as f32 * 2.0 * DT), DT).translation.x;
        }
        let target = frames as f32 * 2.0 * DT;
        assert!(target - last < 0.1, "{last} of {target}");
    }

    #[test]
    fn rotation_stays_normalized() {
        let mut filter = PoseFilter::default();
        let turned =
            |angle| Affine3A::from_rotation_translation(Quat::from_rotation_y(angle), Vec3::ZERO);
        filter.update(turned(0.0), DT);
        for i in 1..200 {
            let rotation = Quat::from_affine3(&filter.update(turned(i as f32 * 0.05), DT));
            assert!(rotation.is_normalized());
        }
    }

    #[test]
    fn reset_takes_the_next_pose_as_it_is() {
        let mut filter = PoseFilter::default();
        filter.update(pose(0.0), DT);
        filter.reset();
        assert_eq!(filter.update(pose(5.0), DT).translation.x, 5.0);
    }

    #[test]
    fn a_held_fist_opens() {
        let mut gate = gate();
        assert!(!gate.update(1.0, FIST, DT));
        assert!(run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS + DT));
    }

    #[test]
    fn a_spike_does_not_open() {
        let mut gate = gate();
        assert!(!run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS / 2.0));
        assert!(!gate.update(0.0, FIST, DT));
        assert!(!run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS / 2.0));
    }

    #[test]
    fn a_pointing_hand_does_not_open() {
        assert!(!run(&mut gate(), 1.0, POINTING, 2.0));
    }

    #[test]
    fn a_half_bent_index_does_not_open() {
        assert!(!run(&mut gate(), 1.0, Some(0.5), 2.0));
    }

    #[test]
    fn without_joints_a_held_fist_still_opens() {
        assert!(run(&mut gate(), 1.0, None, GRASP_HOLD_SECS + DT));
    }

    #[test]
    fn stays_open_down_to_the_release_threshold() {
        let mut gate = gate();
        run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS + DT);
        assert!(gate.update(0.75, FIST, DT));
        assert!(!gate.update(0.65, FIST, DT));
    }

    #[test]
    fn extending_the_index_ends_a_grab() {
        let mut gate = gate();
        run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS + DT);
        // between the two limits a grab holds, so joint jitter doesn't drop it
        assert!(gate.update(1.0, Some(0.4), DT));
        assert!(!gate.update(1.0, POINTING, DT));
    }

    #[test]
    fn lost_joints_keep_a_grab() {
        let mut gate = gate();
        run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS + DT);
        assert!(gate.update(1.0, None, DT));
    }

    #[test]
    fn reset_closes() {
        let mut gate = gate();
        run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS + DT);
        gate.reset();
        assert!(!gate.update(1.0, FIST, DT));
    }
}
