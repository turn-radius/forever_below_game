//! Books: deterministic rolls, every option showing across seeds, finite grounded buffers.

use armory::cond::{Condition, ConditionState};
use armory::families::Generator;
use armory::families::writing::{BookOpts, Centre, Closure, Cover, Edge, LIMITS, Metal, Writing, WritingKind, WritingState};
use armory::{ItemMesh, Part};

fn books(range: std::ops::RangeInclusive<u32>) -> Vec<WritingState> {
    range.map(|s| Writing::roll_as(s, WritingKind::Book)).collect()
}

fn built() -> impl Iterator<Item = (WritingState, ItemMesh)> {
    (1..=256u32).step_by(5).map(|seed| {
        let s = Writing::roll_as(seed, WritingKind::Book);
        let m = Writing::build(&s);
        (s, m)
    })
}

fn part<'a>(m: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    m.parts.iter().find(|p| p.name == name)
}

fn bounds<'a>(parts: impl IntoIterator<Item = &'a Part>) -> ([f32; 3], [f32; 3]) {
    parts.into_iter().flat_map(|p| p.positions.iter()).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

#[test]
fn books_roll_the_same_and_stay_in_range() {
    for s in books(1..=300) {
        assert_eq!(s.book, Writing::roll_as(s.seed, WritingKind::Book).book);
        let bad = s.out_of_range(&LIMITS);
        assert!(bad.is_empty(), "seed {}: {bad:?}", s.seed);
    }
    assert_eq!(Writing::roll(7).book, Writing::roll(7).book);
}

#[test]
fn old_saves_without_a_book_still_load() {
    let json = serde_json::to_value(Writing::roll(5)).unwrap();
    let mut obj = json.as_object().unwrap().clone();
    obj.remove("book");
    let s: WritingState = serde_json::from_value(obj.into()).unwrap();
    assert_eq!(s.book, BookOpts::default());
}

#[test]
fn every_book_option_appears_in_the_first_256_seeds() {
    let all = books(1..=256);
    let any = |f: &dyn Fn(&BookOpts) -> bool| all.iter().any(|s| f(&s.book));
    for c in Cover::ALL {
        assert!(any(&|b| b.cover == c), "no {c:?} cover");
    }
    for e in Edge::ALL {
        assert!(any(&|b| b.edge == e), "no {e:?} edge");
    }
    for c in Closure::ALL {
        assert!(any(&|b| b.closure == c), "no {c:?} closure");
    }
    for c in Centre::ALL {
        assert!(any(&|b| b.centre == c), "no {c:?} centre");
    }
    for m in Metal::ALL {
        assert!(any(&|b| b.metal == m), "no {m:?} fittings");
    }
    for n in 0..=6 {
        assert!(any(&|b| b.bands == n), "no book with {n} bands");
    }
    assert!(any(&|b| b.corners) && any(&|b| !b.corners));
    assert!(any(&|b| b.chain) && any(&|b| b.ribbon) && any(&|b| b.border));
    assert!(any(&|b| b.open) && any(&|b| !b.open));
    assert!(any(&|b| b.open && b.fan >= 2));
    assert!(any(&|b| b.warp > 0.4));
    assert!(any(&|b| b.length < 1.9) && any(&|b| b.length > 3.5));
}

#[test]
fn nouns() {
    let mut s = WritingState::default();
    s.piece.kind = WritingKind::Book;
    s.book = BookOpts { length: 2.4, ..BookOpts::default() };
    assert_eq!(Writing::noun(&s), "Book");
    s.book.length = 1.5;
    assert_eq!(Writing::noun(&s), "Pocket book");
    s.book.length = 3.6;
    s.book.corners = true;
    assert_eq!(Writing::noun(&s), "Grimoire");
    s.book.chain = true;
    assert_eq!(Writing::noun(&s), "Chained tome");
    s.book = BookOpts { cover: Cover::Cloth, thick: 0.6, ..BookOpts::default() };
    assert_eq!(Writing::noun(&s), "Ledger");
    s.book = BookOpts { cover: Cover::Wood, ..BookOpts::default() };
    assert_eq!(Writing::noun(&s), "Tome");
}

#[test]
fn every_part_is_finite_drawable_with_outline_normals() {
    for (s, mesh) in built() {
        assert!(part(&mesh, "cover").is_some() && part(&mesh, "pages").is_some(), "seed {}", s.seed);
        for p in &mesh.parts {
            p.validate().unwrap_or_else(|e| panic!("seed {}: {e}", s.seed));
            assert!(!p.indices.is_empty() && p.material.is_some(), "seed {}: {}", s.seed, p.name);
            assert!(p.positions.iter().flatten().all(|v| v.is_finite()), "seed {}: {} not finite", s.seed, p.name);
            assert_eq!(p.outline_normals.len(), p.positions.len(), "seed {}: {} has no outline normals", s.seed, p.name);
            assert!(p.outline_normals.iter().flatten().all(|v| v.is_finite()), "seed {}: {}", s.seed, p.name);
        }
    }
}

#[test]
fn books_rest_on_the_ground_centred_and_sized() {
    for (s, mesh) in built() {
        let b = &s.book;
        let (lo, hi) = bounds(mesh.parts.iter().filter(|p| !matches!(p.name, "chain" | "ribbon")));
        assert!(lo[1].abs() < 0.12, "seed {}: body bottom at {}", s.seed, lo[1]);
        let (all_lo, _) = bounds(&mesh.parts);
        assert!(all_lo[1].abs() < 0.03, "seed {}: lowest point at {}", s.seed, all_lo[1]);
        let (dx, dz) = (hi[0] - lo[0], hi[2] - lo[2]);
        assert!(dx >= dz - 1e-3, "seed {}: long side is not along X ({dx} x {dz})", s.seed);
        assert!(((lo[0] + hi[0]) * 0.5).abs() < 0.05 && ((lo[2] + hi[2]) * 0.5).abs() < 0.05, "seed {}: off centre", s.seed);
        let span = if b.open { dx } else { dx.max(dz) };
        assert!(span >= b.length - 0.2, "seed {}: {span} short of length {}", s.seed, b.length);
        assert!(span <= (b.length * b.aspect * 2.0).max(b.length) + 0.4, "seed {}: {span} too long", s.seed);
        assert!((1.2..=8.4).contains(&span) && hi[1] < 3.0, "seed {}", s.seed);
    }
}

#[test]
fn closed_books_are_as_thick_as_asked_and_open_ones_show_ink() {
    for (s, mesh) in built() {
        let b = &s.book;
        let (lo, hi) = bounds(mesh.parts.iter().filter(|p| matches!(p.name, "cover" | "pages")));
        if !b.open {
            let thick = hi[1] - lo[1];
            let want = b.thick + 2.0 * b.board;
            assert!((want - 0.1..=want + 0.2).contains(&thick), "seed {}: {thick} vs {want}", s.seed);
            assert!(part(&mesh, "spine").is_some());
            assert!(part(&mesh, "ink").is_none());
        } else if s.piece.lines > 0 {
            let ink = part(&mesh, "ink").unwrap_or_else(|| panic!("seed {}: open book without ink", s.seed));
            let up = ink.normals.iter().filter(|n| n[1] > 0.0).count();
            assert!(up * 10 > ink.normals.len() * 8, "seed {}: ink faces down", s.seed);
        }
        assert_eq!(part(&mesh, "chain").is_some(), b.chain, "seed {}", s.seed);
        assert_eq!(part(&mesh, "ribbon").is_some(), b.ribbon, "seed {}", s.seed);
    }
}

#[test]
fn wear_scuffs_the_cover_and_browns_the_pages() {
    let mut s = Writing::roll_as(11, WritingKind::Book);
    s.book = BookOpts { open: false, cover: Cover::Cloth, ..s.book };
    let clean = Writing::build(&s);
    s.cond = Condition::of(ConditionState::GraveFind);
    let worn = Writing::build(&s);
    assert!(part(&clean, "scuff").is_none() && part(&worn, "scuff").is_some());
    assert!(part(&worn, "fray").is_some());
    assert_ne!(part(&clean, "pages").unwrap().tint, part(&worn, "pages").unwrap().tint);
}

#[test]
fn leaves_show_as_many_separate_ridges() {
    let mut s = Writing::roll_as(3, WritingKind::Book);
    s.book = BookOpts { open: false, thick: 0.8, edge: Edge::Plain, ..s.book };
    let thick = Writing::build(&s);
    s.book.thick = 0.2;
    let thin = Writing::build(&s);
    let (a, b) = (part(&thick, "pages").unwrap(), part(&thin, "pages").unwrap());
    assert!(a.positions.len() > b.positions.len() * 2);
}
