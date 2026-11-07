//! oneVR: what a tracked hand does with a grab, where a controller has its thumbstick.

use glam::{Affine3A, Vec3A};

/// [`HandDepth`]: the panel's distance from the hand changes with the hand's reach to this power,
/// so a short push or pull moves a far panel a long way
const DEPTH_POWER: f32 = 2.0;
/// The least reach that counts, in m, which keeps the ratio sane with a hand at the head
const MIN_REACH: f32 = 0.08;
/// The least spread of two hands that counts, in m
const MIN_SPREAD: f32 = 0.05;
/// The scale a stretch keeps a panel within, as the thumbstick's resize
const SCALE_LIMITS: [f32; 2] = [0.1, 20.0];

/// How far a hand is from the head, level: lowering or raising the hand doesn't push or pull
fn reach(hmd: &Affine3A, hand: &Affine3A) -> f32 {
    let mut to_hand = hand.translation - hmd.translation;
    to_hand.y = 0.0;
    to_hand.length().max(MIN_REACH)
}

/// Pushing a grabbing hand away from the head pushes the panel away, pulling it back pulls the
/// panel in, as a thumbstick does for a controller
#[derive(Debug, Clone, Copy)]
pub struct HandDepth {
    reach: f32,
    offset: Vec3A,
}

impl HandDepth {
    /// `offset` is the grab's, from the hand to the panel
    pub fn new(hmd: &Affine3A, hand: &Affine3A, offset: Vec3A) -> Self {
        Self {
            reach: reach(hmd, hand),
            offset,
        }
    }

    /// The grab's offset for the hand where it is now
    pub fn offset(&self, hmd: &Affine3A, hand: &Affine3A) -> Vec3A {
        self.offset * (reach(hmd, hand) / self.reach).powf(DEPTH_POWER)
    }
}

/// The other hand's grab on the panel one hand holds: spreading the hands grows the panel,
/// bringing them together shrinks it
#[derive(Debug, Clone, Copy)]
pub struct HandStretch {
    /// The pointer that holds the panel
    pub holder: usize,
    spread: f32,
}

impl HandStretch {
    pub fn new(holder: usize, a: &Affine3A, b: &Affine3A) -> Self {
        Self {
            holder,
            spread: spread(a, b),
        }
    }

    /// The scale change since the last update
    pub fn update(&mut self, a: &Affine3A, b: &Affine3A) -> f32 {
        let spread = spread(a, b);
        let factor = spread / self.spread;
        self.spread = spread;
        factor
    }
}

fn spread(a: &Affine3A, b: &Affine3A) -> f32 {
    a.translation.distance(b.translation).max(MIN_SPREAD)
}

/// Scales `transform` by `factor` within [`SCALE_LIMITS`], returning the factor it took
pub fn scale_by(transform: &mut Affine3A, factor: f32) -> f32 {
    let scale = transform.x_axis.length();
    if scale <= f32::EPSILON {
        return 1.0;
    }
    let factor = (scale * factor).clamp(SCALE_LIMITS[0], SCALE_LIMITS[1]) / scale;
    transform.matrix3 = transform.matrix3.mul_scalar(factor);
    factor
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn at(x: f32, y: f32, z: f32) -> Affine3A {
        Affine3A::from_translation(Vec3::new(x, y, z))
    }

    #[test]
    fn a_still_hand_keeps_the_offset() {
        let depth = HandDepth::new(&at(0.0, 1.6, 0.0), &at(0.0, 1.3, -0.4), Vec3A::NEG_Z);
        assert_eq!(
            depth.offset(&at(0.0, 1.6, 0.0), &at(0.0, 1.3, -0.4)),
            Vec3A::NEG_Z
        );
    }

    #[test]
    fn pushing_moves_the_panel_further_than_the_hand() {
        let hmd = at(0.0, 1.6, 0.0);
        let depth = HandDepth::new(&hmd, &at(0.0, 1.3, -0.3), Vec3A::NEG_Z * 2.0);
        let pushed = depth.offset(&hmd, &at(0.0, 1.3, -0.45));
        // 15 cm of the hand, 2.5 m of the panel
        assert!((pushed.z + 4.5).abs() < 1e-4, "{pushed}");
    }

    #[test]
    fn pulling_brings_the_panel_in() {
        let hmd = at(0.0, 1.6, 0.0);
        let depth = HandDepth::new(&hmd, &at(0.0, 1.3, -0.4), Vec3A::NEG_Z * 2.0);
        assert!(depth.offset(&hmd, &at(0.0, 1.3, -0.2)).z > -1.0);
    }

    #[test]
    fn raising_the_hand_does_not_push() {
        let hmd = at(0.0, 1.6, 0.0);
        let depth = HandDepth::new(&hmd, &at(0.0, 1.0, -0.4), Vec3A::NEG_Z);
        assert_eq!(depth.offset(&hmd, &at(0.0, 1.8, -0.4)), Vec3A::NEG_Z);
    }

    #[test]
    fn a_hand_at_the_head_stays_finite() {
        let hmd = at(0.0, 1.6, 0.0);
        let depth = HandDepth::new(&hmd, &hmd, Vec3A::NEG_Z);
        assert!(depth.offset(&hmd, &at(0.0, 1.6, -0.5)).is_finite());
    }

    #[test]
    fn spreading_grows() {
        let mut stretch = HandStretch::new(0, &at(-0.1, 0.0, 0.0), &at(0.1, 0.0, 0.0));
        let factor = stretch.update(&at(-0.2, 0.0, 0.0), &at(0.2, 0.0, 0.0));
        assert!((factor - 2.0).abs() < 1e-5);
        // from where the last update left it
        let factor = stretch.update(&at(-0.1, 0.0, 0.0), &at(0.1, 0.0, 0.0));
        assert!((factor - 0.5).abs() < 1e-5);
    }

    #[test]
    fn touching_hands_stay_finite() {
        let mut stretch = HandStretch::new(0, &at(0.0, 0.0, 0.0), &at(0.0, 0.0, 0.0));
        assert!(
            stretch
                .update(&at(0.0, 0.0, 0.0), &at(0.3, 0.0, 0.0))
                .is_finite()
        );
    }

    #[test]
    fn scale_stays_within_the_limits() {
        let mut transform = Affine3A::from_scale(Vec3::splat(15.0));
        let factor = scale_by(&mut transform, 2.0);
        assert!((transform.x_axis.length() - SCALE_LIMITS[1]).abs() < 1e-4);
        assert!((factor - 20.0 / 15.0).abs() < 1e-5);
        scale_by(&mut transform, 1e-6);
        assert!((transform.x_axis.length() - SCALE_LIMITS[0]).abs() < 1e-4);
    }
}
