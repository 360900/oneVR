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

/// [`WristFilter`]: each of its two stages' time constant, in s. A wrist that moves in the
/// tracker's steps moves within 15% of an even speed, for about 40 ms of lag.
const WRIST_TAU: f32 = 0.02;
/// [`WristFilter`]: a wrist that moved this far in a frame was found again elsewhere, in m
const WRIST_JUMP: f32 = 0.15;

/// Smooths a wrist that the watch sits on. The runtime tracks hands slower than the display and
/// the server interpolates between its samples at uneven times, so the pose moves in uneven
/// steps, which a watch held to it shows. Two low-pass stages in a row leave a smooth motion.
#[derive(Default)]
pub(super) struct WristFilter {
    stages: Option<[(Vec3, Quat); 2]>,
}

impl WristFilter {
    pub(super) fn update(&mut self, wrist: Option<Affine3A>, secs: f32) -> Option<Affine3A> {
        let Some(wrist) = wrist else {
            self.stages = None;
            return None;
        };
        let sample = (
            Vec3::from(wrist.translation),
            Quat::from_affine3(&wrist).normalize(),
        );
        let stages = match &mut self.stages {
            Some(stages) if stages[1].0.distance(sample.0) <= WRIST_JUMP => stages,
            stages => stages.insert([sample; 2]),
        };
        let step = secs / (WRIST_TAU + secs);
        let mut input = sample;
        for stage in stages.iter_mut() {
            stage.0 = stage.0.lerp(input.0, step);
            stage.1 = stage.1.slerp(input.1, step).normalize();
            input = *stage;
        }
        let (position, rotation) = stages[1];
        Some(Affine3A::from_rotation_translation(rotation, position))
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
    height: Range,
    palm_down: Range,
    reach: Range,
}

/// TEMP: where a hand is and how it's turned, for telling typing from grabbing
#[derive(Clone, Copy)]
pub(super) struct HandPlace {
    /// The wrist's height below the head, in m, negative below
    pub height: f32,
    /// The palm's normal against straight down, 1 facing the floor
    pub palm_down: f32,
    /// The wrist's distance from the head, level, in m
    pub reach: f32,
}

impl HandPlace {
    pub(super) fn new(hmd: Affine3A, wrist: Option<Affine3A>, palm: Option<Palm>) -> Option<Self> {
        let (wrist, palm) = (wrist?, palm?);
        let to_wrist = Vec3::from(wrist.translation - hmd.translation);
        Some(Self {
            height: to_wrist.y,
            palm_down: palm.normal.dot(Vec3::NEG_Y),
            reach: Vec3::new(to_wrist.x, 0.0, to_wrist.z).length(),
        })
    }
}

/// TEMP: a value's least and greatest in a log's second
#[derive(Default, Clone, Copy)]
struct Range(Option<(f32, f32)>);

impl Range {
    fn add(&mut self, value: f32) {
        self.0 = Some(self.0.map_or((value, value), |(min, max)| {
            (min.min(value), max.max(value))
        }));
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some((min, max)) => write!(f, "{min:.2}..{max:.2}"),
            None => write!(f, "-"),
        }
    }
}

impl HandLog {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update(
        &mut self,
        side: usize,
        raw: f32,
        gated: f32,
        ready: bool,
        bend: Option<f32>,
        place: Option<HandPlace>,
        secs: f32,
    ) {
        self.secs += secs;
        if let Some(place) = place {
            self.height.add(place.height);
            self.palm_down.add(place.palm_down);
            self.reach.add(place.reach);
        }
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
        if grab && !self.was_grab {
            log::info!(
                "TEMP grab {}: bend={bend:.2?} height={:.2?} palm_down={:.2?} reach={:.2?}",
                ["left", "right"][side],
                place.map(|p| p.height),
                place.map(|p| p.palm_down),
                place.map(|p| p.reach),
            );
        }
        self.was_fist = fist;
        self.was_grab = grab;
        if self.secs >= 1.0 {
            log::info!(
                "TEMP hand {}: grasp_max={:.2} bend={:.2?}..{:.2?} joints={}% ready={}% fists={} grabs={} height={} palm_down={} reach={}",
                ["left", "right"][side],
                self.grasp_max,
                self.bend_min,
                self.bend_max,
                self.joint_frames * 100 / self.frames,
                self.ready_frames * 100 / self.frames,
                self.fists,
                self.grabs,
                self.height,
                self.palm_down,
                self.reach,
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

/// [`ClapDetector`]: palms this close have met, in m between the palm joints
const CLAP_DISTANCE: f32 = 0.08;
/// [`ClapDetector`]: hands have to close at least this fast, in m/s, so bringing them together
/// slowly isn't a clap
const CLAP_SPEED: f32 = 0.6;
/// [`ClapDetector`]: how long a fast approach counts toward a clap, in s; the tracking of hands
/// that overlap often lags the moment they meet
const CLAP_APPROACH_SECS: f32 = 0.2;
/// [`ClapDetector`]: the palms' normals have to be at least this opposed, -1 facing each other
const CLAP_FACING: f32 = -0.5;
/// [`ClapDetector`]: hands this far apart may clap again, in m; hands part less between the
/// claps of a quick double clap
const CLAP_REARM_DISTANCE: f32 = 0.15;
/// [`DoubleClap`]: the second clap has to come this soon after the first, in s
const DOUBLE_CLAP_SECS: f32 = 0.7;

/// Where a hand's palm is and which way it faces
#[derive(Clone, Copy)]
pub(super) struct Palm {
    position: Vec3,
    /// Out of the palm, away from the back of the hand
    normal: Vec3,
}

impl Palm {
    /// None without a valid palm joint
    pub(super) fn new(joints: &xr::HandJointLocations) -> Option<Self> {
        let palm = &joints[xr::HandJoint::PALM];
        let valid =
            xr::SpaceLocationFlags::POSITION_VALID | xr::SpaceLocationFlags::ORIENTATION_VALID;
        if !palm.location_flags.contains(valid) {
            return None;
        }
        let (p, o) = (palm.pose.position, palm.pose.orientation);
        let rotation = Quat::from_xyzw(o.x, o.y, o.z, o.w);
        Some(Self {
            position: Vec3::new(p.x, p.y, p.z),
            // XR_EXT_hand_tracking: +Y points out of the back of the hand
            normal: rotation * Vec3::NEG_Y,
        })
    }
}

/// The wrist joint's pose, None without a valid one. XR_EXT_hand_tracking: -Z points toward the
/// fingers, +Y out of the back of the hand.
pub(super) fn wrist_pose(joints: &xr::HandJointLocations) -> Option<Affine3A> {
    let wrist = &joints[xr::HandJoint::WRIST];
    let valid = xr::SpaceLocationFlags::POSITION_VALID | xr::SpaceLocationFlags::ORIENTATION_VALID;
    if !wrist.location_flags.contains(valid) {
        return None;
    }
    let (p, o) = (wrist.pose.position, wrist.pose.orientation);
    Some(Affine3A::from_rotation_translation(
        Quat::from_xyzw(o.x, o.y, o.z, o.w),
        Vec3::new(p.x, p.y, p.z),
    ))
}

/// A hand as a clap sees it: None for a hand that isn't the input, isn't tracked, or isn't open
pub(super) type ClapHand = Option<Palm>;

/// Whether an index finger is out, see [`index_bend`]: a fist or a pointing hand doesn't clap
pub(super) fn is_open(bend: Option<f32>) -> bool {
    bend.is_some_and(|bend| bend > INDEX_EXTENDED)
}

/// A clap of two open hands: the palms face each other, close fast and meet. One clap counts
/// once, the hands have to part before the next.
#[derive(Default)]
pub(super) struct ClapDetector {
    last_distance: Option<f32>,
    /// Time left in which the last fast approach counts
    approach_secs: f32,
    clapped: bool,
}

impl ClapDetector {
    pub(super) fn update(&mut self, hands: [ClapHand; 2], dt: f32) -> bool {
        let [Some(left), Some(right)] = hands else {
            // a lost hand, as hands that overlap often are, keeps the approach for a moment
            self.last_distance = None;
            self.approach_secs = (self.approach_secs - dt).max(0.0);
            return false;
        };
        let distance = left.position.distance(right.position);
        if distance > CLAP_REARM_DISTANCE {
            self.clapped = false;
        }

        let speed = self
            .last_distance
            .map_or(0.0, |last| (last - distance) / dt);
        self.last_distance = Some(distance);
        self.approach_secs = if speed >= CLAP_SPEED {
            CLAP_APPROACH_SECS
        } else {
            (self.approach_secs - dt).max(0.0)
        };

        let to_right = (right.position - left.position).normalize_or_zero();
        let facing = left.normal.dot(right.normal) <= CLAP_FACING
            && left.normal.dot(to_right) > 0.0
            && right.normal.dot(-to_right) > 0.0;
        let clap = !self.clapped && self.approach_secs > 0.0 && distance <= CLAP_DISTANCE && facing;
        if clap {
            self.clapped = true;
            self.approach_secs = 0.0;
        }
        clap
    }
}

/// TEMP: how hands that come close look to [`ClapDetector`], logged once a second while they do
#[derive(Default)]
pub(super) struct ClapLog {
    secs: f32,
    /// Frames with both palms within 30 cm
    close: u32,
    /// Of those, frames with both hands open
    open: u32,
    nearest: Option<f32>,
    /// At the nearest: the normals' dot, -1 facing
    facing: f32,
    last: Option<f32>,
    speed_max: f32,
}

impl ClapLog {
    pub(super) fn update(&mut self, palms: [Option<Palm>; 2], open: [bool; 2], dt: f32) {
        self.secs += dt;
        if let [Some(left), Some(right)] = palms {
            let distance = left.position.distance(right.position);
            if let Some(last) = self.last {
                self.speed_max = self.speed_max.max((last - distance) / dt);
            }
            self.last = Some(distance);
            if distance < 0.3 {
                self.close += 1;
                self.open += u32::from(open[0] && open[1]);
                if self.nearest.is_none_or(|nearest| distance < nearest) {
                    self.nearest = Some(distance);
                    self.facing = left.normal.dot(right.normal);
                }
            }
        } else {
            self.last = None;
        }
        if self.secs >= 1.0 {
            if self.close > 0 {
                log::info!(
                    "TEMP clap: close={} open={} nearest={:.3?} facing={:.2} speed_max={:.2}",
                    self.close,
                    self.open,
                    self.nearest,
                    self.facing,
                    self.speed_max,
                );
            }
            *self = Self::default();
        }
    }
}

/// Two claps in quick succession, so applause or a single clap doesn't count
#[derive(Default)]
pub(super) struct DoubleClap {
    clap: ClapDetector,
    /// Time left for the second clap, while the first counts
    second_secs: f32,
}

impl DoubleClap {
    pub(super) fn update(&mut self, hands: [ClapHand; 2], dt: f32) -> bool {
        self.second_secs = (self.second_secs - dt).max(0.0);
        if !self.clap.update(hands, dt) {
            return false;
        }
        log::info!("TEMP clap: one clap, second={}", self.second_secs > 0.0);
        if self.second_secs > 0.0 {
            // a third clap starts over
            self.second_secs = 0.0;
            return true;
        }
        self.second_secs = DOUBLE_CLAP_SECS;
        false
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

    /// `bend` is None when the joints say nothing: a fist doesn't start then, as a hand the headset
    /// is losing reports fists that aren't, and two of them toggled the dashboard. A grab that
    /// holds stays held.
    pub(super) fn update(&mut self, value: f32, bend: Option<f32>, dt: f32) -> bool {
        if self.open {
            let extended = bend.is_some_and(|bend| bend > INDEX_EXTENDED);
            self.open = value >= self.thresholds[0] && !extended;
            self.held_secs = 0.0;
            return self.open;
        }
        let curled = bend.is_some_and(|bend| bend <= INDEX_CURLED);
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
    fn a_stepping_wrist_moves_evenly() {
        // 1 m/s, sampled at 50 Hz and shown at 72 Hz
        let mut filter = WristFilter::default();
        let mut speeds = Vec::new();
        let mut last = 0.0;
        for frame in 0..200 {
            let time = frame as f32 * DT;
            let sample = (time * 50.0).floor() / 50.0;
            let x = filter.update(Some(pose(sample)), DT).unwrap().translation.x;
            if frame > 50 {
                speeds.push((x - last) / DT);
            }
            last = x;
        }
        let (min, max) = speeds
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(min > 0.85 && max < 1.15, "{min}..{max}");
    }

    #[test]
    fn a_found_again_wrist_is_taken_as_it_is() {
        let mut filter = WristFilter::default();
        filter.update(Some(pose(0.0)), DT);
        assert_eq!(
            filter.update(Some(pose(1.0)), DT).unwrap().translation.x,
            1.0
        );
        assert!(filter.update(None, DT).is_none());
        assert_eq!(
            filter.update(Some(pose(2.0)), DT).unwrap().translation.x,
            2.0
        );
    }

    #[test]
    fn without_joints_a_fist_does_not_open() {
        assert!(!run(&mut gate(), 1.0, None, 2.0));
    }

    #[test]
    fn losing_the_joints_restarts_a_fist() {
        let mut gate = gate();
        assert!(!run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS * 0.75));
        assert!(!gate.update(1.0, None, DT));
        assert!(!run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS * 0.75));
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

    /// Open palms facing each other, `gap` m apart on the x axis
    fn palms(gap: f32) -> [ClapHand; 2] {
        [
            Some(Palm {
                position: Vec3::new(-gap / 2.0, 1.2, -0.4),
                normal: Vec3::X,
            }),
            Some(Palm {
                position: Vec3::new(gap / 2.0, 1.2, -0.4),
                normal: Vec3::NEG_X,
            }),
        ]
    }

    /// Brings the palms from `from` to `to` m apart at `speed` m/s, true if any frame clapped
    fn close(clap: &mut ClapDetector, from: f32, to: f32, speed: f32) -> bool {
        let frames = ((from - to) / speed / DT).ceil() as usize;
        let mut clapped = false;
        for i in 0..=frames {
            let gap = (from - i as f32 * speed * DT).max(to);
            clapped |= clap.update(palms(gap), DT);
        }
        clapped
    }

    #[test]
    fn a_clap_counts_once() {
        let mut clap = ClapDetector::default();
        assert!(close(&mut clap, 0.4, 0.04, 1.5));
        // held together
        for _ in 0..50 {
            assert!(!clap.update(palms(0.04), DT));
        }
    }

    #[test]
    fn hands_part_before_the_next_clap() {
        let mut clap = ClapDetector::default();
        assert!(close(&mut clap, 0.4, 0.04, 1.5));
        assert!(!close(&mut clap, 0.15, 0.04, 1.5));
        clap.update(palms(0.3), DT);
        assert!(close(&mut clap, 0.3, 0.04, 1.5));
    }

    #[test]
    fn slowly_joined_hands_do_not_clap() {
        assert!(!close(&mut ClapDetector::default(), 0.4, 0.04, 0.2));
    }

    #[test]
    fn a_clap_survives_lost_tracking_at_contact() {
        let mut clap = ClapDetector::default();
        assert!(!close(&mut clap, 0.4, 0.1, 1.5));
        clap.update([None, None], DT);
        clap.update([None, None], DT);
        assert!(clap.update(palms(0.05), DT));
    }

    #[test]
    fn palms_facing_away_do_not_clap() {
        let mut clap = ClapDetector::default();
        let mut clapped = false;
        for i in 0..20 {
            let mut hands = palms((0.4 - i as f32 * 0.02).max(0.04));
            for hand in hands.iter_mut().flatten() {
                hand.normal = -hand.normal;
            }
            clapped |= clap.update(hands, DT);
        }
        assert!(!clapped);
    }

    #[test]
    fn palms_facing_down_do_not_clap() {
        let mut clap = ClapDetector::default();
        let mut clapped = false;
        for i in 0..20 {
            let mut hands = palms((0.4 - i as f32 * 0.02).max(0.04));
            for hand in hands.iter_mut().flatten() {
                hand.normal = Vec3::NEG_Y;
            }
            clapped |= clap.update(hands, DT);
        }
        assert!(!clapped);
    }

    #[test]
    fn one_hand_does_not_clap() {
        let mut clap = ClapDetector::default();
        let [left, _] = palms(0.04);
        for _ in 0..20 {
            assert!(!clap.update([left, None], DT));
        }
    }

    /// Claps once from `from` m apart, returning what the double clap said
    fn clap_once(double: &mut DoubleClap, from: f32) -> bool {
        let speed = 1.5;
        let frames = ((from - 0.04) / speed / DT).ceil() as usize;
        let mut fired = false;
        for i in 0..=frames {
            let gap = (from - i as f32 * speed * DT).max(0.04);
            fired |= double.update(palms(gap), DT);
        }
        fired
    }

    fn wait(double: &mut DoubleClap, gap: f32, secs: f32) {
        for _ in 0..(secs / DT).ceil() as usize {
            assert!(!double.update(palms(gap), DT));
        }
    }

    #[test]
    fn a_double_clap_fires() {
        let mut double = DoubleClap::default();
        assert!(!clap_once(&mut double, 0.3));
        wait(&mut double, 0.2, 0.1);
        assert!(clap_once(&mut double, 0.2));
    }

    #[test]
    fn a_single_clap_does_not_fire() {
        let mut double = DoubleClap::default();
        assert!(!clap_once(&mut double, 0.3));
        wait(&mut double, 0.3, 2.0);
    }

    #[test]
    fn claps_too_far_apart_do_not_fire() {
        let mut double = DoubleClap::default();
        assert!(!clap_once(&mut double, 0.3));
        wait(&mut double, 0.3, DOUBLE_CLAP_SECS + 0.1);
        assert!(!clap_once(&mut double, 0.3));
    }

    #[test]
    fn a_third_clap_starts_over() {
        let mut double = DoubleClap::default();
        clap_once(&mut double, 0.3);
        wait(&mut double, 0.2, 0.05);
        assert!(clap_once(&mut double, 0.2));
        wait(&mut double, 0.2, 0.05);
        assert!(!clap_once(&mut double, 0.2));
    }

    #[test]
    fn reset_closes() {
        let mut gate = gate();
        run(&mut gate, 1.0, FIST, GRASP_HOLD_SECS + DT);
        gate.reset();
        assert!(!gate.update(1.0, FIST, DT));
    }
}
