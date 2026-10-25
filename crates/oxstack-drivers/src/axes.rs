//! Mapping a sensor's axes onto a board's.
//!
//! An IMU reports in its own frame, fixed by how the package is soldered.
//! [`AxisMap`] turns that into the frame a program wants (say, +Z out of the
//! screen), and [`AxisMap::from_gravity`] works the mapping out from three
//! readings taken with the board held in known poses, so it never has to be
//! guessed from a layout drawing.

/// One signed axis of the sensor's own frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Axis {
    /// The sensor's +X.
    PosX,
    /// The sensor's −X.
    NegX,
    /// The sensor's +Y.
    PosY,
    /// The sensor's −Y.
    NegY,
    /// The sensor's +Z.
    PosZ,
    /// The sensor's −Z.
    NegZ,
}

impl Axis {
    /// Index into a `[x, y, z]` reading.
    pub const fn index(self) -> usize {
        match self {
            Axis::PosX | Axis::NegX => 0,
            Axis::PosY | Axis::NegY => 1,
            Axis::PosZ | Axis::NegZ => 2,
        }
    }

    /// Whether the axis points the opposite way from the sensor's.
    pub const fn is_negative(self) -> bool {
        matches!(self, Axis::NegX | Axis::NegY | Axis::NegZ)
    }

    /// The same axis, pointing the other way.
    pub const fn flipped(self) -> Self {
        match self {
            Axis::PosX => Axis::NegX,
            Axis::NegX => Axis::PosX,
            Axis::PosY => Axis::NegY,
            Axis::NegY => Axis::PosY,
            Axis::PosZ => Axis::NegZ,
            Axis::NegZ => Axis::PosZ,
        }
    }

    /// The component of `v` along this axis.
    pub const fn pick(self, v: [f32; 3]) -> f32 {
        let c = v[self.index()];
        if self.is_negative() { -c } else { c }
    }

    /// The axis along which `v` mostly points, if it clearly points along
    /// one: that component must be at least `min` in magnitude and at least
    /// twice the next largest. For a resting accelerometer in g, `min = 0.7`
    /// accepts up to about 25° of tilt.
    pub fn dominant(v: [f32; 3], min: f32) -> Option<Self> {
        let mag = [v[0].abs(), v[1].abs(), v[2].abs()];
        let (i, &largest) = mag.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
        let second = mag
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, m)| *m)
            .fold(0.0f32, f32::max);
        if largest < min || largest < 2.0 * second {
            return None;
        }
        let positive = [Axis::PosX, Axis::PosY, Axis::PosZ][i];
        Some(if v[i] < 0.0 {
            positive.flipped()
        } else {
            positive
        })
    }

    /// Rust source for this value, e.g. `Axis::NegY`.
    pub const fn source(self) -> &'static str {
        match self {
            Axis::PosX => "Axis::PosX",
            Axis::NegX => "Axis::NegX",
            Axis::PosY => "Axis::PosY",
            Axis::NegY => "Axis::NegY",
            Axis::PosZ => "Axis::PosZ",
            Axis::NegZ => "Axis::NegZ",
        }
    }
}

/// Which sensor axis each board axis reads.
///
/// ```
/// use oxstack_drivers::axes::{Axis, AxisMap};
///
/// // Sensor rotated 90° clockwise on the board: board +X is sensor −Y.
/// let map = AxisMap { x: Axis::NegY, y: Axis::PosX, z: Axis::PosZ };
/// assert_eq!(map.apply([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
/// assert!(map.is_right_handed());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AxisMap {
    /// The sensor axis the board's +X reads.
    pub x: Axis,
    /// The sensor axis the board's +Y reads.
    pub y: Axis,
    /// The sensor axis the board's +Z reads.
    pub z: Axis,
}

/// Why [`AxisMap::from_gravity`] could not decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum GravityError {
    /// This pose's reading does not point clearly along one sensor axis
    /// (0 = `x_up`, 1 = `y_up`, 2 = `z_up`). Hold the board straighter.
    Unclear(u8),
    /// Two poses picked the same sensor axis: the board was held the same
    /// way twice.
    Repeated,
}

impl AxisMap {
    /// Board frame equals sensor frame.
    pub const IDENTITY: Self = Self {
        x: Axis::PosX,
        y: Axis::PosY,
        z: Axis::PosZ,
    };

    /// Translates a sensor-frame vector into the board frame.
    pub const fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        [self.x.pick(v), self.y.pick(v), self.z.pick(v)]
    }

    /// Whether every sensor axis is used exactly once.
    pub const fn is_permutation(&self) -> bool {
        let (x, y, z) = (self.x.index(), self.y.index(), self.z.index());
        x != y && y != z && x != z
    }

    /// Whether the map is a rotation (determinant +1) rather than a
    /// reflection. A sensor soldered flat on a board always gives a
    /// rotation; a reflection means one pose was misread.
    pub const fn is_right_handed(&self) -> bool {
        if !self.is_permutation() {
            return false;
        }
        // Parity of the permutation: even for xyz, yzx, zxy.
        let even = matches!(
            (self.x.index(), self.y.index(), self.z.index()),
            (0, 1, 2) | (1, 2, 0) | (2, 0, 1)
        );
        let flips =
            self.x.is_negative() as u8 + self.y.is_negative() as u8 + self.z.is_negative() as u8;
        even == flips.is_multiple_of(2)
    }

    /// Works the map out from gravity. Each argument is one resting
    /// accelerometer reading (in g, sensor frame) with the named **board**
    /// axis pointing straight up, so that it reads about +1 g:
    ///
    /// - `x_up`: board stood on its left edge (+X up),
    /// - `y_up`: board stood on its bottom edge (+Y up),
    /// - `z_up`: board lying flat, face up (+Z up).
    ///
    /// ```
    /// use oxstack_drivers::axes::{Axis, AxisMap};
    ///
    /// let map = AxisMap::from_gravity(
    ///     [0.02, -0.98, 0.05],   // +X up: sensor −Y reads gravity
    ///     [0.99, 0.01, -0.03],   // +Y up: sensor +X
    ///     [-0.04, 0.0, 1.01],    // +Z up: sensor +Z
    /// )
    /// .unwrap();
    /// assert_eq!(map, AxisMap { x: Axis::NegY, y: Axis::PosX, z: Axis::PosZ });
    /// ```
    pub fn from_gravity(
        x_up: [f32; 3],
        y_up: [f32; 3],
        z_up: [f32; 3],
    ) -> Result<Self, GravityError> {
        const MIN: f32 = 0.7;
        let x = Axis::dominant(x_up, MIN).ok_or(GravityError::Unclear(0))?;
        let y = Axis::dominant(y_up, MIN).ok_or(GravityError::Unclear(1))?;
        let z = Axis::dominant(z_up, MIN).ok_or(GravityError::Unclear(2))?;
        let map = Self { x, y, z };
        if !map.is_permutation() {
            return Err(GravityError::Repeated);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_a_rotation() {
        assert!(AxisMap::IDENTITY.is_right_handed());
        assert_eq!(AxisMap::IDENTITY.apply([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn all_four_flat_rotations_are_right_handed() {
        for (x, y) in [
            (Axis::PosX, Axis::PosY),
            (Axis::NegY, Axis::PosX),
            (Axis::NegX, Axis::NegY),
            (Axis::PosY, Axis::NegX),
        ] {
            let m = AxisMap {
                x,
                y,
                z: Axis::PosZ,
            };
            assert!(m.is_right_handed(), "{m:?}");
        }
        // Mirror: swap X and Y without a sign change.
        let mirror = AxisMap {
            x: Axis::PosY,
            y: Axis::PosX,
            z: Axis::PosZ,
        };
        assert!(!mirror.is_right_handed());
        // Flipping one axis of a rotation makes a reflection.
        let flipped = AxisMap {
            x: Axis::NegX,
            y: Axis::PosY,
            z: Axis::PosZ,
        };
        assert!(!flipped.is_right_handed());
    }

    #[test]
    fn dominant_axis_tolerates_tilt_but_not_ambiguity() {
        assert_eq!(Axis::dominant([0.0, 0.0, 0.98], 0.7), Some(Axis::PosZ));
        assert_eq!(Axis::dominant([-0.9, 0.4, 0.1], 0.7), Some(Axis::NegX));
        assert_eq!(Axis::dominant([0.7, 0.7, 0.0], 0.7), None); // 45°
        assert_eq!(Axis::dominant([0.3, 0.1, 0.1], 0.7), None); // free fall?
    }

    #[test]
    fn gravity_calibration_rejects_repeated_poses() {
        let flat = [0.0, 0.0, 1.0];
        assert_eq!(
            AxisMap::from_gravity(flat, [0.0, 1.0, 0.0], flat),
            Err(GravityError::Repeated)
        );
        assert_eq!(
            AxisMap::from_gravity([0.6, 0.6, 0.0], [0.0, 1.0, 0.0], flat),
            Err(GravityError::Unclear(0))
        );
    }

    #[test]
    fn gravity_calibration_round_trips() {
        let truth = AxisMap {
            x: Axis::NegX,
            y: Axis::NegY,
            z: Axis::PosZ,
        };
        // Sensor reading when board axis `i` points up: the sensor vector
        // whose board-frame image is e_i. Build it by inverting the map.
        let sensor_for = |board_axis: Axis| {
            let mut v = [0.0f32; 3];
            v[board_axis.index()] = if board_axis.is_negative() { -1.0 } else { 1.0 };
            v
        };
        let map = AxisMap::from_gravity(
            sensor_for(truth.x),
            sensor_for(truth.y),
            sensor_for(truth.z),
        )
        .unwrap();
        assert_eq!(map, truth);
    }
}
