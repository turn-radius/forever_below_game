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
        ("slab chamfered star", bevelled_slab(&star, &[], &SlabOptions::chamfered(0.4, 0.1))),
        ("slab rounded holed disc", bevelled_slab(&disc(1.0, 48), &[disc(0.4, 24)], &SlabOptions::rounded(0.3, 0.1))),
        ("slab flat", bevelled_slab(&star, &[], &SlabOptions::chamfered(0.4, 0.0))),
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
fn thick_sheet_closes_its_edges_and_faces_out() {
    // An open-topped cup with a pole at the bottom: front surface out, back surface in, a
    // rim on top. Every face of the outer wall points away from the axis.
    let cup: Vec<Vec<V3>> = (0..=8)
        .map(|r| {
            let y = r as f32 / 8.0;
            let radius = (y * 4.0).min(1.0);
            (0..16).map(|c| {
                let (s, co) = (TAU * c as f32 / 16.0).sin_cos();
                [radius * co, y, -radius * s]
            }).collect()
        })
        .collect();
    let options = GridOptions { wrap_around: true, ..GridOptions::default() };
    let normals = grid_normals(&cup, true, false);
    assert!(normals[0][0][1] < -0.9, "pole normal {:?} should point down", normals[0][0]);
    assert!(vec3::dot(normals[6][0], [1.0, 0.0, 0.0]) > 0.9);
    let sheet = thick_sheet(&cup, &options, 0.05);
    let p = part(sheet);
    assert_front_faces_agree(&p);
    // Rim faces point up, and the mesh is closed: every edge is shared by exactly two faces.
    let mut edges: HashMap<([i32; 3], [i32; 3]), i32> = HashMap::new();
    let key = |v: V3| v.map(|c| (c * 1e4).round() as i32);
    for tri in p.indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let (a, b) = (key(p.positions[tri[k] as usize]), key(p.positions[tri[(k + 1) % 3] as usize]));
            *edges.entry(if a < b { (a, b) } else { (b, a) }).or_default() += 1;
        }
    }
    assert!(edges.values().all(|&n| n == 2), "thick sheet has open edges");
}

#[test]
fn extrude_smooth_keeps_its_sharp_corners() {
    // A half disc: round on top, sharp where the arc meets the flat base.
    let mut outline: Vec<[f32; 2]> = (0..=12).map(|i| {
        let a = PI * i as f32 / 12.0;
        [a.cos(), a.sin()]
    }).collect();
    outline.reverse();
    let sharp: Vec<bool> = (0..outline.len()).map(|i| i == 0 || i == outline.len() - 1).collect();
    let p = part(extrude_smooth(&outline, &sharp, 2.0));
    assert_front_faces_agree(&p);
    assert_outward(&p, [0.0, 0.4, 0.0]);
    // At the sharp corner (1, 0) some vertex faces straight down (the base) and another not.
    let at_corner: Vec<V3> = p.positions.iter().zip(&p.normals).filter(|(q, _)| (q[0] - 1.0).abs() < 1e-5 && q[1].abs() < 1e-5 && q[2].abs() > 0.99).map(|(_, n)| *n).collect();
    assert!(at_corner.iter().any(|n| n[1] < -0.99), "{at_corner:?}");
    assert!(at_corner.iter().any(|n| n[1] > -0.5 && n[2].abs() < 0.5), "{at_corner:?}");
}

#[test]
fn place_basis_maps_up_and_forward() {
    let mut m = cuboid([0.1, 1.0, 0.1]);
    m.translate([0.0, 0.5, 0.2]).place_basis([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
    let (lo, hi) = m.bounds().unwrap();
    assert!((hi[0] - 2.0).abs() < 1e-4 && (lo[2] + 0.25).abs() < 1e-4, "{lo:?} {hi:?}");
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

#[test]
fn sector_closed_by_its_cut_faces_faces_outward() {
    let profile = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let (start, end) = (-0.5f32, 0.5f32);
    let mut wedge = lathe_sector(&profile, &LatheOptions::new(8, Shading::SMOOTH), start, end);
    let (rind, core) = profile_face(&profile, 0.1);
    for (mut face, far) in [(rind.clone(), false), (core.clone(), false), (rind, true), (core, true)] {
        if far {
            face.flip().rotate_y(end);
        } else {
            face.rotate_y(start);
        }
        wedge.append(&face);
    }
    let p = part(wedge);
    assert_front_faces_agree(&p);
    assert_outward(&p, [0.5, 0.5, 0.0]);
}

#[test]
fn rind_face_splits_the_area() {
    let square = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
    let (rind, core) = rind_face(&square, 0.25);
    let area = |m: &MeshBuilder| -> f32 {
        m.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| m.positions[i as usize]);
                vec3::length(vec3::cross(vec3::sub(b, a), vec3::sub(c, a))) * 0.5
            })
            .sum()
    };
    assert!((area(&core) - 1.5 * 1.5).abs() < 1e-4, "core {}", area(&core));
    assert!((area(&rind) + area(&core) - 4.0).abs() < 1e-4);
    assert!(rind.normals.iter().chain(&core.normals).all(|n| *n == [0.0, 0.0, 1.0]));
}

#[test]
fn shell_and_clip_profiles() {
    let outer = [[0.0, 0.0], [1.0, 0.0], [1.0, 2.0]];
    let shell = shell_profile(&outer, 0.1);
    assert_eq!(shell.first(), Some(&[0.0, 0.0]));
    assert!((shell[outer.len()][0] - 0.9).abs() < 1e-6 && shell[outer.len()][1] == 2.0, "flat lip a wall wide");
    let inside = shell_inside(&shell, outer.len());
    assert!(inside[0][0].abs() < 1e-6 && (inside[0][1] - 0.1).abs() < 1e-5, "inside floor {:?}", inside[0]);
    let liquid = clip_profile(&inside, 1.0);
    assert_eq!(liquid.last(), Some(&[0.0, 1.0]));
    assert!(liquid.iter().all(|q| q[1] <= 1.0 + 1e-6));
    assert!((profile_radius_at(&inside, 1.0, false).unwrap() - 0.9).abs() < 1e-5);
    assert!((profile_height_at(&[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0]], 0.5).unwrap() - 1.0).abs() < 1e-6);
    part(lathe(&shell, &LatheOptions::default()));
}

#[test]
fn carved_hollow_lies_on_the_ball() {
    let ball = uv_sphere(1.0, 48, 32);
    let bite = Hollow::bite(vec3::ZERO, [1.0, 0.2, 0.0], 1.0, 0.6);
    let (kept, carved) = carve_hollows(&ball, &[bite]);
    assert!(!carved.is_empty());
    for p in &carved.positions {
        assert!((vec3::distance(*p, bite.center) - bite.radius).abs() < 1e-3, "{p:?} off the hollow");
    }
    for p in &kept.positions {
        assert!(vec3::distance(*p, bite.center) >= bite.radius - 1e-3, "{p:?} left inside the hollow");
    }
    assert_front_faces_agree(&part(kept));
    assert_front_faces_agree(&part(carved));
}

fn disc(r: f32, n: usize) -> Vec<[f32; 2]> {
    (0..n).map(|i| {
        let (s, c) = (TAU * i as f32 / n as f32).sin_cos();
        [r * c, r * s]
    }).collect()
}

/// Every edge, with vertices welded by position, borders exactly two triangles.
fn assert_watertight(mesh: &MeshBuilder) {
    let key = |p: V3| p.map(|c| (c * 1e4).round() as i64);
    let mut edges: HashMap<([i64; 3], [i64; 3]), i32> = HashMap::new();
    for tri in mesh.indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let (a, b) = (key(mesh.positions[tri[k] as usize]), key(mesh.positions[tri[(k + 1) % 3] as usize]));
            // Directed: a closed, consistently wound surface uses each edge once each way.
            *edges.entry((a, b)).or_default() += 1;
            *edges.entry((b, a)).or_default() -= 1;
        }
    }
    let open = edges.values().filter(|&&v| v != 0).count();
    assert_eq!(open, 0, "{open} unmatched directed edges");
}

#[test]
fn slabs_are_closed_and_face_outward() {
    for (depth, bevel, rounded) in [(0.3, 0.1, true), (0.3, 0.1, false), (0.3, 0.0, false)] {
        let options = if rounded { SlabOptions::rounded(depth, bevel) } else { SlabOptions::chamfered(depth, bevel) };
        let mesh = bevelled_slab(&disc(1.0, 40), &[], &options);
        assert_watertight(&mesh);
        assert_outward(&part(mesh.clone()), vec3::ZERO);
        let holed = bevelled_slab(&disc(1.0, 40), &[disc(0.3, 20)], &options);
        assert_watertight(&holed);
        let (lo, hi) = mesh.bounds().unwrap();
        assert!((hi[2] - lo[2] - depth).abs() < 1e-5 && (hi[0] - 1.0).abs() < 1e-5);
    }
}

#[test]
fn slab_walls_are_smooth_round_a_disc_and_creased_at_a_square() {
    let round = bevelled_slab(&disc(1.0, 40), &[], &SlabOptions::chamfered(0.3, 0.0));
    let square = bevelled_slab(&[[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]], &[], &SlabOptions::chamfered(0.3, 0.0));
    let wall_normals_at = |m: &MeshBuilder, p: V3| -> Vec<V3> {
        m.positions.iter().zip(&m.normals).filter(|(q, n)| vec3::distance(**q, p) < 1e-5 && n[2].abs() < 1e-3).map(|(_, n)| *n).collect()
    };
    let r = wall_normals_at(&round, [1.0, 0.0, 0.15]);
    assert!(r.len() >= 2 && r.iter().all(|n| vec3::distance(*n, r[0]) < 1e-5), "disc wall should be smooth: {r:?}");
    let s = wall_normals_at(&square, [1.0, 1.0, 0.15]);
    assert!(s.iter().any(|n| (n[0] - 1.0).abs() < 1e-5) && s.iter().any(|n| (n[1] - 1.0).abs() < 1e-5), "square corner should crease: {s:?}");
}
