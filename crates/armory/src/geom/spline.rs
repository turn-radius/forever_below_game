//! Curves for sweeps: centripetal Catmull-Rom splines, arc-length resampling, helices, and
//! rotation-minimizing frames along a path.
//!
//! - Centripetal parameterization (alpha = 0.5) never forms cusps or self-intersections within
//!   a segment: Yuksel, Schaefer and Keyser, "Parameterization and applications of Catmull-Rom
//!   curves" (2011), <https://www.cemyuksel.com/research/catmullrom_param/>.
//! - Frames use the double reflection method: Wang, Juttler, Zheng and Liu, "Computation of
//!   rotation minimizing frames" (ACM TOG 27(1), 2008),
//!   <https://www.microsoft.com/en-us/research/publication/computation-rotation-minimizing-frames/>.

use super::vec3::{self, Mat3, V3};

/// A Catmull-Rom spline through every control point.
#[derive(Clone, Debug, PartialEq)]
pub struct CatmullRom {
    pub points: Vec<V3>,
    /// The last point joins back to the first.
    pub closed: bool,
    /// 0 uniform, 0.5 centripetal (the default), 1 chordal.
    pub alpha: f32,
}

impl CatmullRom {
    /// An open centripetal spline through `points`.
    pub fn centripetal(points: Vec<V3>) -> Self {
        CatmullRom { points, closed: false, alpha: 0.5 }
    }

    pub fn with_closed(mut self, closed: bool) -> Self {
        self.closed = closed;
        self
    }

    pub fn segment_count(&self) -> usize {
        match self.points.len() {
            0 | 1 => 0,
            n if self.closed => n,
            n => n - 1,
        }
    }

    /// The four control points around segment `i`, with mirrored phantom points at open ends.
    fn controls(&self, i: usize) -> [V3; 4] {
        let n = self.points.len();
        let at = |k: isize| -> V3 {
            if self.closed {
                self.points[k.rem_euclid(n as isize) as usize]
            } else if k < 0 {
                vec3::sub(vec3::scale(self.points[0], 2.0), self.points[1])
            } else if k as usize >= n {
                vec3::sub(vec3::scale(self.points[n - 1], 2.0), self.points[n - 2])
            } else {
                self.points[k as usize]
            }
        };
        let i = i as isize;
        [at(i - 1), at(i), at(i + 1), at(i + 2)]
    }

    /// Point at `t` in `0..=1` over the whole curve (each segment gets an equal share of `t`).
    pub fn point(&self, t: f32) -> V3 {
        let segments = self.segment_count();
        match self.points.len() {
            0 => return vec3::ZERO,
            1 => return self.points[0],
            _ => {}
        }
        let x = t.clamp(0.0, 1.0) * segments as f32;
        let i = (x.floor() as usize).min(segments - 1);
        self.segment_point(i, x - i as f32)
    }

    /// Barry-Goldman pyramidal evaluation of segment `i` at local `u` in `0..=1`.
    fn segment_point(&self, i: usize, u: f32) -> V3 {
        let [p0, p1, p2, p3] = self.controls(i);
        let knot = |a: V3, b: V3| vec3::distance(a, b).powf(self.alpha).max(1e-6);
        let t0 = 0.0;
        let t1 = t0 + knot(p0, p1);
        let t2 = t1 + knot(p1, p2);
        let t3 = t2 + knot(p2, p3);
        let t = t1 + (t2 - t1) * u;
        let mix = |a: V3, b: V3, ta: f32, tb: f32| vec3::lerp(a, b, (t - ta) / (tb - ta));
        let a1 = mix(p0, p1, t0, t1);
        let a2 = mix(p1, p2, t1, t2);
        let a3 = mix(p2, p3, t2, t3);
        let b1 = mix(a1, a2, t0, t2);
        let b2 = mix(a2, a3, t1, t3);
        mix(b1, b2, t1, t2)
    }

    /// `per_segment` points per segment, through every control point. An open curve includes
    /// its last point; a closed one stops before repeating its first.
    pub fn sample(&self, per_segment: usize) -> Vec<V3> {
        let per_segment = per_segment.max(1);
        let segments = self.segment_count();
        if segments == 0 {
            return self.points.clone();
        }
        let mut out = Vec::with_capacity(segments * per_segment + 1);
        for i in 0..segments {
            for k in 0..per_segment {
                out.push(self.segment_point(i, k as f32 / per_segment as f32));
            }
        }
        if !self.closed {
            out.push(self.points[self.points.len() - 1]);
        }
        out
    }

    /// `count` points spaced evenly by arc length.
    pub fn sample_evenly(&self, count: usize) -> Vec<V3> {
        resample_by_length(&self.sample(24), count, self.closed)
    }
}

/// Total length of a polyline (including the closing edge when `closed`).
pub fn polyline_length(points: &[V3], closed: bool) -> f32 {
    let open: f32 = points.windows(2).map(|w| vec3::distance(w[0], w[1])).sum();
    match (closed, points.first(), points.last()) {
        (true, Some(&a), Some(&b)) => open + vec3::distance(a, b),
        _ => open,
    }
}

/// `count` points spaced evenly by arc length along a polyline. An open result runs end to
/// end; a closed one stops one step before returning to the start.
pub fn resample_by_length(points: &[V3], count: usize, closed: bool) -> Vec<V3> {
    if points.len() < 2 || count < 2 {
        return points.iter().copied().take(count.max(1)).collect();
    }
    let mut path = points.to_vec();
    if closed {
        path.push(points[0]);
    }
    let mut cumulative = vec![0.0f32; path.len()];
    for i in 1..path.len() {
        cumulative[i] = cumulative[i - 1] + vec3::distance(path[i - 1], path[i]);
    }
    let total = cumulative[cumulative.len() - 1];
    let steps = if closed { count } else { count - 1 };
    let mut out = Vec::with_capacity(count);
    let mut seg = 0;
    for k in 0..count {
        let target = total * k as f32 / steps as f32;
        while seg + 2 < path.len() && cumulative[seg + 1] < target {
            seg += 1;
        }
        let span = cumulative[seg + 1] - cumulative[seg];
        let t = if span > 1e-9 { ((target - cumulative[seg]) / span).clamp(0.0, 1.0) } else { 0.0 };
        out.push(vec3::lerp(path[seg], path[seg + 1], t));
    }
    out
}

/// A helix up +Y from the origin: `turns` turns over `height`, starting at `[radius, 0, 0]`
/// and turning from +X toward -Z. `samples` points in all. For cord wraps and coiled springs.
pub fn helix(radius: f32, height: f32, turns: f32, samples: usize) -> Vec<V3> {
    let samples = samples.max(2);
    (0..samples)
        .map(|i| {
            let t = i as f32 / (samples - 1) as f32;
            let (s, c) = (std::f32::consts::TAU * turns * t).sin_cos();
            [radius * c, height * t, -radius * s]
        })
        .collect()
}

/// An orthonormal frame on a path: `binormal = tangent x normal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: V3,
    pub tangent: V3,
    pub normal: V3,
    pub binormal: V3,
}

impl Frame {
    /// Item-space position of `[x, y]` in the frame's normal/binormal plane.
    pub fn place(&self, offset: [f32; 2]) -> V3 {
        vec3::add(self.origin, vec3::add(vec3::scale(self.normal, offset[0]), vec3::scale(self.binormal, offset[1])))
    }
}

fn path_tangents(path: &[V3], closed: bool) -> Vec<V3> {
    let n = path.len();
    let raw: Vec<Option<V3>> = (0..n)
        .map(|i| {
            let prev = if i > 0 { path[i - 1] } else if closed { path[n - 1] } else { path[i] };
            let next = if i + 1 < n { path[i + 1] } else if closed { path[0] } else { path[i] };
            vec3::try_normalize(vec3::sub(next, prev))
        })
        .collect();
    // Repeated points borrow the nearest real tangent (earlier first, then later).
    let fallback = raw.iter().flatten().next().copied().unwrap_or(vec3::Y);
    let mut last = fallback;
    raw.into_iter()
        .map(|t| {
            last = t.unwrap_or(last);
            last
        })
        .collect()
}

/// Reflects frame (`r`, `t`) at `x0` onto `x1` with tangent `t1` (one double-reflection step).
fn transport(x0: V3, r: V3, t: V3, x1: V3, t1: V3) -> V3 {
    let v1 = vec3::sub(x1, x0);
    let c1 = vec3::dot(v1, v1);
    if c1 < 1e-12 {
        return vec3::normalize_or(vec3::sub(r, vec3::scale(t1, vec3::dot(r, t1))), r);
    }
    let r_l = vec3::sub(r, vec3::scale(v1, 2.0 / c1 * vec3::dot(v1, r)));
    let t_l = vec3::sub(t, vec3::scale(v1, 2.0 / c1 * vec3::dot(v1, t)));
    let v2 = vec3::sub(t1, t_l);
    let c2 = vec3::dot(v2, v2);
    let r1 = if c2 < 1e-12 { r_l } else { vec3::sub(r_l, vec3::scale(v2, 2.0 / c2 * vec3::dot(v2, r_l))) };
    // Re-orthonormalize against drift.
    vec3::try_normalize(vec3::sub(r1, vec3::scale(t1, vec3::dot(r1, t1)))).unwrap_or_else(|| vec3::any_perpendicular(t1))
}

/// Rotation-minimizing frames along a path: the cross-section does not twist or flip as the
/// path bends. `up` picks the first frame's normal (projected off the tangent); without it a
/// stable perpendicular is chosen. A closed path spreads the leftover twist evenly so the
/// last frame meets the first.
pub fn rotation_minimizing_frames(path: &[V3], up: Option<V3>, closed: bool) -> Vec<Frame> {
    let n = path.len();
    if n == 0 {
        return Vec::new();
    }
    let tangents = path_tangents(path, closed);
    let t0 = tangents[0];
    let first = up
        .and_then(|u| vec3::try_normalize(vec3::sub(u, vec3::scale(t0, vec3::dot(u, t0)))))
        .unwrap_or_else(|| vec3::any_perpendicular(t0));
    let mut normals = Vec::with_capacity(n);
    normals.push(first);
    for i in 1..n {
        let r = transport(path[i - 1], normals[i - 1], tangents[i - 1], path[i], tangents[i]);
        normals.push(r);
    }
    if closed && n > 2 {
        let back = transport(path[n - 1], normals[n - 1], tangents[n - 1], path[0], t0);
        let angle = vec3::dot(vec3::cross(back, first), t0).atan2(vec3::dot(back, first));
        for (i, normal) in normals.iter_mut().enumerate().skip(1) {
            *normal = Mat3::from_axis_angle(tangents[i], angle * i as f32 / n as f32).transform(*normal);
        }
    }
    path.iter()
        .zip(tangents)
        .zip(normals)
        .map(|((&origin, tangent), normal)| Frame { origin, tangent, normal, binormal: vec3::cross(tangent, normal) })
        .collect()
}
