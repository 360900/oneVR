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

/// TEMP: what a hand's grasp values look like, logged once a second for tuning the gate
#[derive(Default)]
pub(super) struct HandLog {
    secs: f32,
    frames: u32,
    joint_frames: u32,
    ready_frames: u32,
    grasp_max: f32,
    bend_min: Option<f32>,
    bend_max: Option<f32>,
    /// Times the raw value reached the press threshold
    fists: u32,
    /// Times the gate let it through
    grabs: u32,
    was_fist: bool,
    was_grab: bool,
}

impl HandLog {
    pub(super) fn update(
        &mut self,
        side: usize,
        raw: f32,
        gated: f32,
        ready: bool,
        bend: Option<f32>,
        secs: f32,
    ) {
        self.secs += secs;
        self.frames += 1;
        self.ready_frames += u32::from(ready);
        self.grasp_max = self.grasp_max.max(raw);
        if let Some(bend) = bend {
            self.joint_frames += 1;
            self.bend_min = Some(self.bend_min.map_or(bend, |min| min.min(bend)));
            self.bend_max = Some(self.bend_max.map_or(bend, |max| max.max(bend)));
        }
        let fist = raw >= 0.9;
        let grab = gated > 0.0;
        self.fists += u32::from(fist && !self.was_fist);
        self.grabs += u32::from(grab && !self.was_grab);
        self.was_fist = fist;
        self.was_grab = grab;
        if self.secs >= 1.0 {
            log::info!(
                "TEMP hand {}: grasp_max={:.2} bend={:.2?}..{:.2?} joints={}% ready={}% fists={} grabs={}",
                ["left", "right"][side],
                self.grasp_max,
                self.bend_min,
                self.bend_max,
                self.joint_frames * 100 / self.frames,
                self.ready_frames * 100 / self.frames,
                self.fists,
                self.grabs,
            );
            *self = Self {
                was_fist: fist,
                was_grab: grab,
                ..Self::default()
            };
        }
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

/// [`FingerPinch`]: a fingertip this close to the thumb's may start a pinch, in m
const FINGER_PINCH_PRESS: f32 = 0.02;
/// [`FingerPinch`]: a fingertip this far from the thumb's ends a pinch, in m
const FINGER_PINCH_RELEASE: f32 = 0.035;
/// [`FingerPinch`]: the index tip has to be this far from the thumb's, or it is the index's pinch
const INDEX_APART: f32 = 0.04;
/// [`FingerPinch`]: a pinch has to last this long, which drops the joints' spikes
const FINGER_PINCH_HOLD_SECS: f32 = 0.04;

/// The tips of the fingers that pinch, None without valid joints
#[derive(Clone, Copy)]
pub(super) struct FingerTips {
    thumb: Vec3,
    index: Vec3,
    middle: Vec3,
    ring: Vec3,
}

impl FingerTips {
    pub(super) fn new(joints: &xr::HandJointLocations) -> Option<Self> {
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
        Some(Self {
            thumb: position(xr::HandJoint::THUMB_TIP)?,
            index: position(xr::HandJoint::INDEX_TIP)?,
            middle: position(xr::HandJoint::MIDDLE_TIP)?,
            ring: position(xr::HandJoint::RING_TIP)?,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Finger {
    Middle,
    Ring,
}

/// A pinch of the thumb with the middle or the ring finger. The runtime only reports the index's,
/// so this one comes from the joints.
struct FingerPinch {
    finger: Finger,
    held_secs: f32,
    open: bool,
}

impl FingerPinch {
    const fn new(finger: Finger) -> Self {
        Self {
            finger,
            held_secs: 0.0,
            open: false,
        }
    }

    /// `bend` is the index's, see [`index_bend`]. Lost joints keep a pinch as it is: a pinch that
    /// drops and comes back would click twice.
    fn update(&mut self, tips: Option<FingerTips>, bend: Option<f32>, dt: f32) -> bool {
        let Some(tips) = tips else {
            self.held_secs = 0.0;
            return self.open;
        };
        let (own, other) = match self.finger {
            Finger::Middle => (tips.middle, tips.ring),
            Finger::Ring => (tips.ring, tips.middle),
        };
        let distance = tips.thumb.distance(own);
        if self.open {
            self.open = distance < FINGER_PINCH_RELEASE;
            self.held_secs = 0.0;
            return self.open;
        }
        // in a fist the thumb lies near every fingertip, and the index is curled
        let index_out = bend.is_some_and(|bend| bend > INDEX_CURLED);
        let starts = distance <= FINGER_PINCH_PRESS
            && distance < tips.thumb.distance(other)
            && tips.thumb.distance(tips.index) >= INDEX_APART
            && index_out;
        if starts {
            self.held_secs += dt;
            self.open = self.held_secs >= FINGER_PINCH_HOLD_SECS;
        } else {
            self.held_secs = 0.0;
        }
        self.open
    }

    const fn reset(&mut self) {
        self.held_secs = 0.0;
        self.open = false;
    }
}

/// A hand's right click, a thumb and middle finger pinch, and its middle click, a thumb and ring
/// finger pinch
pub(super) struct FingerPinches {
    right: FingerPinch,
    middle: FingerPinch,
}

impl Default for FingerPinches {
    fn default() -> Self {
        Self {
            right: FingerPinch::new(Finger::Middle),
            middle: FingerPinch::new(Finger::Ring),
        }
    }
}

impl FingerPinches {
    /// Whether the hand right clicks and middle clicks. One pinch at a time: the other has to
    /// end before a new one starts.
    pub(super) fn update(
        &mut self,
        tips: Option<FingerTips>,
        bend: Option<f32>,
        dt: f32,
    ) -> (bool, bool) {
        if self.middle.open {
            self.right.reset();
            return (false, self.middle.update(tips, bend, dt));
        }
        let right = self.right.update(tips, bend, dt);
        if right {
            self.middle.reset();
            return (true, false);
        }
        (false, self.middle.update(tips, bend, dt))
    }

    pub(super) const fn reset(&mut self) {
        self.right.reset();
        self.middle.reset();
    }
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

    const INDEX_OUT: Option<f32> = Some(0.9);

    /// Tips with the middle and ring fingers `middle` and `ring` m from the thumb, the index 6 cm
    fn tips(middle: f32, ring: f32) -> Option<FingerTips> {
        Some(FingerTips {
            thumb: Vec3::ZERO,
            index: Vec3::new(0.06, 0.0, 0.0),
            middle: Vec3::new(0.0, middle, 0.0),
            ring: Vec3::new(0.0, 0.0, ring),
        })
    }

    fn pinch(
        pinches: &mut FingerPinches,
        tips: Option<FingerTips>,
        bend: Option<f32>,
        secs: f32,
    ) -> (bool, bool) {
        let mut state = (false, false);
        for _ in 0..(secs / DT).ceil() as usize {
            state = pinches.update(tips, bend, DT);
        }
        state
    }

    const HOLD: f32 = FINGER_PINCH_HOLD_SECS + DT;

    #[test]
    fn a_middle_pinch_right_clicks() {
        let mut pinches = FingerPinches::default();
        assert_eq!(
            pinch(&mut pinches, tips(0.01, 0.05), INDEX_OUT, HOLD),
            (true, false)
        );
        assert_eq!(
            pinch(&mut pinches, tips(0.05, 0.05), INDEX_OUT, DT),
            (false, false)
        );
    }

    #[test]
    fn a_ring_pinch_middle_clicks() {
        let mut pinches = FingerPinches::default();
        assert_eq!(
            pinch(&mut pinches, tips(0.05, 0.01), INDEX_OUT, HOLD),
            (false, true)
        );
    }

    #[test]
    fn the_nearer_finger_pinches() {
        let mut pinches = FingerPinches::default();
        assert_eq!(
            pinch(&mut pinches, tips(0.018, 0.012), INDEX_OUT, HOLD),
            (false, true)
        );
    }

    #[test]
    fn a_short_touch_does_not_pinch() {
        let mut pinches = FingerPinches::default();
        assert_eq!(
            pinch(
                &mut pinches,
                tips(0.01, 0.05),
                INDEX_OUT,
                FINGER_PINCH_HOLD_SECS / 2.0
            ),
            (false, false)
        );
    }

    #[test]
    fn a_fist_does_not_pinch() {
        let mut pinches = FingerPinches::default();
        assert_eq!(
            pinch(&mut pinches, tips(0.01, 0.05), FIST, 1.0),
            (false, false)
        );
        assert_eq!(
            pinch(&mut pinches, tips(0.01, 0.05), None, 1.0),
            (false, false)
        );
    }

    #[test]
    fn the_index_pinch_is_not_a_middle_pinch() {
        let mut pinches = FingerPinches::default();
        let mut close = tips(0.015, 0.05).unwrap();
        close.index = Vec3::new(0.01, 0.0, 0.0);
        assert_eq!(
            pinch(&mut pinches, Some(close), INDEX_OUT, 1.0),
            (false, false)
        );
    }

    #[test]
    fn a_pinch_holds_until_the_release_distance() {
        let mut pinches = FingerPinches::default();
        pinch(&mut pinches, tips(0.01, 0.05), INDEX_OUT, HOLD);
        // and the index or the fist no longer matter
        assert_eq!(pinches.update(tips(0.03, 0.05), FIST, DT), (true, false));
        assert_eq!(pinches.update(tips(0.04, 0.05), FIST, DT), (false, false));
    }

    #[test]
    fn lost_joints_keep_a_pinch() {
        let mut pinches = FingerPinches::default();
        pinch(&mut pinches, tips(0.01, 0.05), INDEX_OUT, HOLD);
        assert_eq!(pinches.update(None, None, DT), (true, false));
    }

    #[test]
    fn one_pinch_at_a_time() {
        let mut pinches = FingerPinches::default();
        pinch(&mut pinches, tips(0.05, 0.01), INDEX_OUT, HOLD);
        // the middle finger joins the ring's pinch
        assert_eq!(
            pinch(&mut pinches, tips(0.005, 0.01), INDEX_OUT, 1.0),
            (false, true)
        );
    }

    #[test]
    fn reset_closes() {
        let mut gate = gate();
        run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS + DT);
        gate.reset();
        assert!(!gate.update(1.0, FIST, DT));
    }
}
