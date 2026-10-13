//! Small vector and matrix helpers over plain arrays, so mesh code stays dependency free.
//! Points and directions are `[f32; 3]` ([`V3`]); call these as `vec3::add(a, b)`.

/// A point or direction in item space.
pub type V3 = [f32; 3];

pub const ZERO: V3 = [0.0, 0.0, 0.0];
pub const X: V3 = [1.0, 0.0, 0.0];
pub const Y: V3 = [0.0, 1.0, 0.0];
pub const Z: V3 = [0.0, 0.0, 1.0];

#[inline]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// Component-wise product.
#[inline]
pub fn mul(a: V3, b: V3) -> V3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

#[inline]
pub fn neg(a: V3) -> V3 {
    [-a[0], -a[1], -a[2]]
}

#[inline]
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[inline]
pub fn length_squared(a: V3) -> f32 {
    dot(a, a)
}

#[inline]
pub fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}

#[inline]
pub fn distance(a: V3, b: V3) -> f32 {
    length(sub(a, b))
}

#[inline]
pub fn lerp(a: V3, b: V3, t: f32) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// `a` scaled to unit length, or `None` when it is too short (or not finite) to have a direction.
#[inline]
pub fn try_normalize(a: V3) -> Option<V3> {
    let len = length(a);
    if len.is_finite() && len > 1e-12 { Some(scale(a, 1.0 / len)) } else { None }
}

/// `a` scaled to unit length, or `fallback` when `a` has no direction.
#[inline]
pub fn normalize_or(a: V3, fallback: V3) -> V3 {
    try_normalize(a).unwrap_or(fallback)
}

/// Any unit vector perpendicular to `a` (stable: picks the world axis least aligned with `a`).
pub fn any_perpendicular(a: V3) -> V3 {
    let axis = if a[0].abs() <= a[1].abs() && a[0].abs() <= a[2].abs() {
        X
    } else if a[1].abs() <= a[2].abs() {
        Y
    } else {
        Z
    };
    normalize_or(cross(a, axis), X)
}

/// Mean of a set of points (the origin for an empty set).
pub fn centroid(points: &[V3]) -> V3 {
    if points.is_empty() {
        return ZERO;
    }
    let sum = points.iter().fold(ZERO, |acc, &p| add(acc, p));
    scale(sum, 1.0 / points.len() as f32)
}

/// A 3x3 matrix stored as three columns, for rotations, scales and bases.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub cols: [V3; 3],
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3 { cols: [X, Y, Z] };

    /// The matrix whose columns are `x`, `y`, `z` (maps the world axes onto them).
    pub fn from_cols(x: V3, y: V3, z: V3) -> Mat3 {
        Mat3 { cols: [x, y, z] }
    }

    pub fn from_scale(s: V3) -> Mat3 {
        Mat3::from_cols([s[0], 0.0, 0.0], [0.0, s[1], 0.0], [0.0, 0.0, s[2]])
    }

    /// Right-handed rotation of `angle` radians about `axis` (Rodrigues' formula).
    pub fn from_axis_angle(axis: V3, angle: f32) -> Mat3 {
        let [x, y, z] = normalize_or(axis, Y);
        let (s, c) = angle.sin_cos();
        let t = 1.0 - c;
        Mat3::from_cols(
            [t * x * x + c, t * x * y + s * z, t * x * z - s * y],
            [t * x * y - s * z, t * y * y + c, t * y * z + s * x],
            [t * x * z + s * y, t * y * z - s * x, t * z * z + c],
        )
    }

    /// The shortest rotation taking direction `from` onto direction `to`.
    pub fn from_rotation_arc(from: V3, to: V3) -> Mat3 {
        let a = normalize_or(from, Y);
        let b = normalize_or(to, Y);
        let c = dot(a, b).clamp(-1.0, 1.0);
        if c > 1.0 - 1e-6 {
            return Mat3::IDENTITY;
        }
        if c < -1.0 + 1e-6 {
            return Mat3::from_axis_angle(any_perpendicular(a), std::f32::consts::PI);
        }
        Mat3::from_axis_angle(cross(a, b), c.acos())
    }

    pub fn transform(&self, v: V3) -> V3 {
        add(add(scale(self.cols[0], v[0]), scale(self.cols[1], v[1])), scale(self.cols[2], v[2]))
    }

    pub fn determinant(&self) -> f32 {
        dot(self.cols[0], cross(self.cols[1], self.cols[2]))
    }

    /// The cofactor matrix: transforms normals correctly under any linear map (it is the
    /// inverse transpose times the determinant, so it also exists for singular maps).
    pub fn cofactor(&self) -> Mat3 {
        let [a, b, c] = self.cols;
        Mat3::from_cols(cross(b, c), cross(c, a), cross(a, b))
    }

    pub fn mul_mat(&self, other: &Mat3) -> Mat3 {
        Mat3::from_cols(self.transform(other.cols[0]), self.transform(other.cols[1]), self.transform(other.cols[2]))
    }
}
