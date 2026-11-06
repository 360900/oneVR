//! oneVR: hand tracking input that the runtime's values alone get wrong.

use glam::Vec3;
use openxr as xr;

/// A fist has to last this long, which drops the runtime's spikes
const GRASP_HOLD_SECS: f32 = 0.12;
/// [`index_bend`] at or below this may start a grab, about 80 degrees
const INDEX_CURLED: f32 = 0.2;
/// [`index_bend`] above this ends a grab, about 50 degrees
const INDEX_EXTENDED: f32 = 0.6;

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
