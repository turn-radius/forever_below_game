use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use super::*;
use crate::geom::vec3;

fn part(mesh: MeshBuilder) -> Part {
    let p = mesh.into_part("test", OutlineMode::Welded);
    p.validate().unwrap();
    p
}

/// Every triangle's winding agrees with its vertices' shading normals.
fn assert_front_faces_agree(p: &Part) {
    for tri in p.indices.as_chunks::<3>().0 {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| i as usize);
        let face = vec3::cross(vec3::sub(p.positions[b], p.positions[a]), vec3::sub(p.positions[c], p.positions[a]));
        if vec3::length(face) < 1e-8 {
            continue;
        }
        let n = vec3::add(vec3::add(p.normals[a], p.normals[b]), p.normals[c]);
        assert!(vec3::dot(face, n) > 0.0, "{}: triangle {tri:?} winds against its normals", p.name);
    }
}

/// For a solid convex about `center`, every face points away from it.
fn assert_outward(p: &Part, center: V3) {
    for tri in p.indices.as_chunks::<3>().0 {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| p.positions[i as usize]);
        let face = vec3::cross(vec3::sub(b, a), vec3::sub(c, a));
        if vec3::length(face) < 1e-8 {
            continue;
        }
        let mid = vec3::scale(vec3::add(vec3::add(a, b), c), 1.0 / 3.0);
        assert!(vec3::dot(face, vec3::sub(mid, center)) > 0.0, "face {tri:?} points inward");
    }
}

fn all_primitives() -> Vec<(&'static str, MeshBuilder)> {
    let path = CatmullRom::centripetal(vec![[0.0, 0.0, 0.0], [0.5, 2.0, 0.2], [-0.4, 4.0, 0.0], [0.0, 6.0, 1.0]]).sample(8);
    let star: Vec<[f32; 2]> = (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { 2.0 } else { 0.9 };
            let (s, c) = (TAU * i as f32 / 10.0).sin_cos();
            [r * c, r * s]
        })
        .collect();
    vec![
        ("lathe smooth", lathe(&[[0.0, 0.0], [1.0, 0.5], [0.6, 2.0], [0.8, 3.0], [0.0, 3.5]], &LatheOptions::new(12, Shading::SMOOTH))),
        ("lathe flat", lathe(&[[0.5, 0.0], [1.0, 0.5], [0.6, 2.0], [0.8, 3.0]], &LatheOptions::new(6, Shading::FLAT))),
        ("lathe faceted", lathe(&[[0.5, 0.0], [1.0, 0.5], [0.6, 2.0], [0.0, 3.0]], &LatheOptions::new(8, Shading::FACETED))),
        ("lathe banded", lathe(&[[0.0, 0.0], [1.0, 0.5], [0.6, 2.0], [0.8, 3.0]], &LatheOptions::new(10, Shading::BANDED))),
        ("tube", tube(&path, 8, &SweepOptions::default(), |t| 0.3 * (1.0 - 0.5 * t))),
        ("tube to a point", tube(&path, 6, &SweepOptions { shading: Shading::FACETED, ..Default::default() }, |t| 0.3 * (1.0 - t))),
        ("wrap", tube(&helix(0.5, 3.0, 5.0, 120), 5, &SweepOptions::default(), |_| 0.06)),
        ("sweep star", sweep(&path, &star, &SweepOptions { twist: PI, shading: Shading::FLAT, ..Default::default() }, |_| 0.2)),
        ("extrude", extrude(&star, &[], 0.3)),
        ("extrude holed", extrude(&[[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]], &[vec![[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]]], 0.5)),
        ("extrude tapered", extrude_tapered(&[[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]], &[vec![[1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]]], 0.6, 0.2)),
        ("tube varying", tube_varying(&path, 12, &SweepOptions::default(), |t, a| 0.3 + 0.1 * (3.0 * a).cos() * t)),
        ("lathe spline profile", lathe(&spline_profile(&[[0.5, 0.0], [1.0, 0.5], [0.6, 2.0], [0.0, 3.0]], 24), &LatheOptions::new(16, Shading::SMOOTH))),
        ("cloth", parametric_surface(8, 12, &GridOptions::default(), |u, v| [u * 4.0, -v * 6.0, (u * 6.0).sin() * 0.3 * v])),
        ("uv sphere", uv_sphere(1.0, 12, 8)),
        ("icosphere", icosphere(1.0, 2)),
        ("gem", icosphere(1.0, 0).flat_shaded()),
        ("spike concave", spike(0.4, 2.0, 8, 1.0, Shading::SMOOTH)),
        ("spike convex", spike(0.4, 2.0, 6, -1.0, Shading::FACETED)),
        ("cylinder", cylinder(0.5, 2.0, 8, Shading::FACETED)),
        ("cuboid", cuboid([1.0, 2.0, 3.0])),
        ("torus", torus(2.0, 0.4, 24, 8, Shading::SMOOTH)),
    ]
}

#[test]
fn every_primitive_is_valid_and_wound_to_its_normals() {
    for (name, mesh) in all_primitives() {
        assert!(!mesh.is_empty(), "{name} is empty");
        let mut p = part(mesh);
        p.name = name;
        assert_front_faces_agree(&p);
    }
}

#[test]
fn tapered_extrusion_thins_toward_max_x() {
    let mesh = extrude_tapered(&[[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]], &[], 0.4, 0.1);
    let depth_at = |x: f32| mesh.positions.iter().filter(|p| (p[0] - x).abs() < 1e-6).map(|p| p[2]).fold(0.0f32, f32::max) * 2.0;
    assert!((depth_at(0.0) - 0.4).abs() < 1e-5);
    assert!((depth_at(2.0) - 0.1).abs() < 1e-5);
}

#[test]
fn polygon_queries() {
    let square = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
    assert!(point_in_polygon([1.0, 1.0], &square));
    assert!(!point_in_polygon([3.0, 1.0], &square));
    assert!((distance_to_polygon([1.0, 0.5], &square) - 0.5).abs() < 1e-6);
}

#[test]
fn primitives_are_deterministic() {
    let a = all_primitives();
    let b = all_primitives();
    for ((name, x), (_, y)) in a.into_iter().zip(b) {
        assert_eq!(part(x), part(y), "{name} differs between runs");
    }
}

#[test]
fn closed_solids_face_outward() {
    assert_outward(&part(uv_sphere(1.0, 10, 6)), vec3::ZERO);
    assert_outward(&part(icosphere(1.0, 1)), vec3::ZERO);
    assert_outward(&part(cuboid([1.0, 2.0, 3.0])), vec3::ZERO);
    assert_outward(&part(cylinder(0.5, 2.0, 6, Shading::FLAT)), [0.0, 1.0, 0.0]);
    assert_outward(&part(spike(0.5, 2.0, 7, 0.0, Shading::SMOOTH)), [0.0, 0.3, 0.0]);
    let mut mirrored = cuboid([1.0, 1.0, 1.0]);
    mirrored.scale([-1.0, 2.0, 1.0]);
    assert_outward(&part(mirrored), vec3::ZERO);
}

#[test]
fn six_facet_lathe_has_hard_normals_but_welded_outline_normals() {
    let p = part(lathe(&[[0.6, 0.0], [1.0, 1.0], [0.7, 2.0]], &LatheOptions::new(6, Shading::FLAT)));
    let mut by_position: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, pos) in p.positions.iter().enumerate() {
        by_position.entry(pos.map(|c| (c * 1e4).round() as i64)).or_default().push(i);
    }
    let mut saw_hard_edge = false;
    for group in by_position.values().filter(|g| g.len() > 1) {
        let first = group[0];
        for &v in &group[1..] {
            if vec3::dot(p.normals[first], p.normals[v]) < 0.99 {
                saw_hard_edge = true;
            }
            assert!(vec3::distance(p.outline_normals[first], p.outline_normals[v]) < 1e-6, "outline normals split at {:?}", p.positions[v]);
        }
    }
    assert!(saw_hard_edge, "a flat 6-facet lathe should have split shading normals");
    // Flat facets: each face's three vertices share one normal.
    for tri in p.indices.as_chunks::<3>().0 {
        let n = p.normals[tri[0] as usize];
        assert!(tri.iter().all(|&i| vec3::distance(p.normals[i as usize], n) < 1e-5));
    }
}

#[test]
fn lathe_poles_shade_along_the_axis() {
    let p = part(uv_sphere(1.0, 12, 8));
    for (pos, n) in p.positions.iter().zip(&p.normals) {
        if pos[0].abs() < 1e-6 && pos[2].abs() < 1e-6 {
            assert!(n[1].abs() > 0.999, "pole normal {n:?}");
        }
        assert!(vec3::dot(*n, *pos) > 0.9, "sphere normal {n:?} at {pos:?}");
    }
}

#[test]
fn quarter_circle_frames_do_not_flip() {
    let path: Vec<V3> = (0..=32)
        .map(|i| {
            let (s, c) = (FRAC_PI_2 * i as f32 / 32.0).sin_cos();
            [2.0 * c, 2.0 * s, 0.0]
        })
        .collect();
    let frames = rotation_minimizing_frames(&path, Some(vec3::Z), false);
    for f in &frames {
        assert!((vec3::length(f.normal) - 1.0).abs() < 1e-4);
        assert!(vec3::dot(f.normal, f.tangent).abs() < 1e-4);
    }
    for w in frames.windows(2) {
        assert!(vec3::dot(w[0].normal, w[1].normal) > 0.9, "frame flipped");
        assert!(vec3::dot(w[0].binormal, w[1].binormal) > 0.9, "frame flipped");
    }
    // A planar curve keeps its out-of-plane normal the whole way.
    assert!(frames.iter().all(|f| f.normal[2] > 0.999));
    part(tube(&path, 8, &SweepOptions::default(), |_| 0.2));
}

#[test]
fn closed_loop_frames_meet_at_the_seam() {
    let path: Vec<V3> = (0..40)
        .map(|i| {
            let a = TAU * i as f32 / 40.0;
            [a.cos() * 2.0, (2.0 * a).sin() * 0.7, a.sin() * 2.0]
        })
        .collect();
    let frames = rotation_minimizing_frames(&path, None, true);
    let (first, last) = (frames[0], frames[frames.len() - 1]);
    assert!(vec3::dot(first.normal, last.normal) > 0.95, "seam twist left over");
}

#[test]
fn catmull_rom_passes_through_its_points() {
    let pts = vec![[0.0, 0.0, 0.0], [1.0, 2.0, 0.0], [3.0, 2.0, 1.0], [4.0, 0.0, 0.0]];
    let curve = CatmullRom::centripetal(pts.clone());
    let samples = curve.sample(10);
    for (k, p) in pts.iter().enumerate() {
        assert!(vec3::distance(samples[k * 10], *p) < 1e-5);
    }
    let even = curve.sample_evenly(20);
    let steps: Vec<f32> = even.windows(2).map(|w| vec3::distance(w[0], w[1])).collect();
    let mean = steps.iter().sum::<f32>() / steps.len() as f32;
    assert!(steps.iter().all(|s| (s - mean).abs() < 0.05 * mean));
}

#[test]
fn triangulation_covers_the_area_minus_holes() {
    let outer = [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [2.0, 1.5], [0.0, 3.0]];
    let holes = vec![vec![[0.5, 0.5], [1.0, 0.5], [1.0, 1.0], [0.5, 1.0]], vec![[2.5, 0.5], [3.5, 0.5], [3.5, 1.5]]];
    let pts: Vec<[f32; 2]> = outer.iter().copied().chain(holes.iter().flatten().copied()).collect();
    let tris = triangulate(&outer, &holes);
    let area: f32 = tris.iter().map(|t| signed_area(&[pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]])).sum();
    let expected = signed_area(&outer) - signed_area(&holes[0]).abs() - signed_area(&holes[1]).abs();
    assert!(tris.iter().all(|t| signed_area(&[pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]]) > 0.0));
    assert!((area - expected).abs() < 1e-4, "area {area} expected {expected}");
}

#[test]
fn double_sided_cloth_keeps_unit_outline_normals() {
    let mut cloth = parametric_surface(4, 4, &GridOptions::default(), |u, v| [u, v, 0.0]);
    cloth.make_double_sided();
    part(cloth);
}

#[test]
fn transforms_keep_normals_unit_and_faces_front() {
    let mut s = spike(0.3, 1.5, 6, 0.5, Shading::FACETED);
    s.align_y_to([1.0, 1.0, 0.0]).scale([1.0, 0.5, 2.0]).translate([0.0, 3.0, 0.0]);
    let p = part(s);
    assert_front_faces_agree(&p);
}

#[test]
fn sweep_scaled_stretches_each_axis() {
    let frames = rotation_minimizing_frames(&[[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 2.0, 0.0]], Some(vec3::X), false);
    let mesh = sweep_scaled(&frames, &regular_polygon(16, 1.0, 0.0), &SweepOptions::default(), |t| [0.4 * (1.0 - t * 0.5), 0.1]);
    let (_, hi) = mesh.bounds().unwrap();
    assert!((hi[0] - 0.4).abs() < 1e-3 && (hi[2] - 0.1).abs() < 1e-3, "{hi:?}");
    let p = part(mesh);
    assert_front_faces_agree(&p);
    assert_outward(&p, [0.0, 1.0, 0.0]);
    // A section shrunk to a point gets no cap.
    part(sweep_scaled(&frames, &regular_polygon(8, 1.0, 0.0), &SweepOptions::default(), |t| [1.0 - t, 1.0 - t]));
}
