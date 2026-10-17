//! Paper notes: every form and option appears, and each builds finite, grounded geometry.

use armory::cond::{Condition, ConditionState};
use armory::families::Generator;
use armory::families::writing::{LIMITS, NoteForm, SEALS, Writing, WritingKind, WritingState};
use armory::{ItemMesh, Part};

fn notes(range: std::ops::RangeInclusive<u32>) -> Vec<WritingState> {
    range.map(|s| Writing::roll_as(s, WritingKind::Note)).collect()
}

fn bounds<'a>(parts: impl IntoIterator<Item = &'a Part>) -> ([f32; 3], [f32; 3]) {
    parts.into_iter().flat_map(|p| p.positions.iter()).fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])
    })
}

fn part<'a>(mesh: &'a ItemMesh, name: &str) -> Option<&'a Part> {
    mesh.parts.iter().find(|p| p.name == name)
}

/// A few notes of each form, built.
fn built() -> Vec<(WritingState, ItemMesh)> {
    let mut out = Vec::new();
    for form in NoteForm::ALL {
        out.extend(notes(1..=256).into_iter().filter(|s| s.note.form == form).take(6).map(|s| {
            let m = Writing::build(&s);
            (s, m)
        }));
    }
    out
}

#[test]
fn note_rolls_are_deterministic_and_in_range() {
    assert_eq!(notes(1..=64), notes(1..=64));
    for s in notes(1..=256) {
        assert!(s.out_of_range(&LIMITS).is_empty(), "seed {}: {:?}", s.seed, s.out_of_range(&LIMITS));
    }
}

#[test]
fn first_256_seeds_cover_every_note_option() {
    let all = notes(1..=256);
    let any = |f: &dyn Fn(&WritingState) -> bool| all.iter().any(f);
    for form in NoteForm::ALL {
        assert!(any(&|s| s.note.form == form), "no {form:?}");
    }
    assert!(any(&|s| s.note.length < 1.0) && any(&|s| s.note.length > 2.2), "no scrap or full sheet");
    assert!(any(&|s| s.note.tear == 0.0) && any(&|s| s.note.tear > 0.35), "no straight or torn edges");
    assert!(any(&|s| s.note.blot > 0.0) && any(&|s| s.note.blood > 0.0));
    assert!(any(&|s| s.note.scrawl <= 2) && any(&|s| s.note.scrawl >= 5));
    for wax in SEALS {
        assert!(any(&|s| s.note.form == NoteForm::Sealed && s.note.seal_color == wax), "no {wax:?} seal");
    }
}

#[test]
fn nouns_read_plainly() {
    let mut s = Writing::roll_as(1, WritingKind::Note);
    s.note.length = 1.6;
    s.note.tear = 0.0;
    for (form, want) in [(NoteForm::Flat, "Note"), (NoteForm::Folded, "Folded note"), (NoteForm::FoldedTwice, "Folded note"), (NoteForm::Sealed, "Sealed letter"), (NoteForm::Crumpled, "Crumpled note")] {
        s.note.form = form;
        assert_eq!(Writing::noun(&s), want);
    }
    s.note.form = NoteForm::Flat;
    s.note.tear = 0.8;
    assert_eq!(Writing::noun(&s), "Torn page");
    s.note.length = 0.7;
    assert_eq!(Writing::noun(&s), "Scrap of paper");
}

#[test]
fn every_form_builds_finite_grounded_geometry_with_outline_normals() {
    for (s, mesh) in built() {
        let tag = format!("seed {} {:?}", s.seed, s.note.form);
        assert!(part(&mesh, "sheet").is_some(), "{tag}: no sheet");
        for p in &mesh.parts {
            p.validate().unwrap_or_else(|e| panic!("{tag}: {e}"));
            assert!(p.positions.iter().flatten().all(|v| v.is_finite()), "{tag}: {} not finite", p.name);
            assert_eq!(p.outline_normals.len(), p.positions.len(), "{tag}: {} lacks outline normals", p.name);
            assert!(p.outline_normals.iter().flatten().all(|v| v.is_finite()), "{tag}: {} outline normals", p.name);
        }
        let (lo, hi) = bounds(&mesh.parts);
        assert!(lo[1].abs() < 0.03 && hi[1] > lo[1], "{tag}: bottom at {}", lo[1]);
        let (dx, dz) = (hi[0] - lo[0], hi[2] - lo[2]);
        assert!(dx >= dz - 1e-3, "{tag}: long side not along X ({dx} x {dz})");
        assert!(((lo[0] + hi[0]) * 0.5).abs() < 0.05 && ((lo[2] + hi[2]) * 0.5).abs() < 0.05, "{tag}: off centre");
        assert!((0.25..3.5).contains(&dx), "{tag}: long side {dx}");
    }
}

#[test]
fn seals_belong_to_sealed_letters_and_ink_faces_up() {
    for (s, mesh) in built() {
        let tag = format!("seed {} {:?}", s.seed, s.note.form);
        assert_eq!(part(&mesh, "seal").is_some(), s.note.form == NoteForm::Sealed, "{tag}");
        if s.note.blood > 0.0 {
            assert!(part(&mesh, "blood").is_some(), "{tag}: no bloodstain");
        }
        if let Some(ink) = part(&mesh, "ink") {
            if s.note.form == NoteForm::Crumpled {
                continue;
            }
            let up = ink.normals.iter().filter(|n| n[1] > 0.0).count();
            assert!(up * 10 > ink.normals.len() * 9, "{tag}: ink faces down");
        }
    }
}

#[test]
fn sizes_run_from_a_scrap_to_a_full_sheet() {
    let mut s = Writing::roll_as(3, WritingKind::Note);
    s.note.form = NoteForm::Flat;
    s.note.tear = 0.0;
    s.piece.size = 1.0;
    let long = |s: &WritingState| {
        let (lo, hi) = bounds(&Writing::build(s).parts);
        (hi[0] - lo[0]).max(hi[2] - lo[2])
    };
    s.note.length = 0.6;
    let scrap = long(&s);
    s.note.length = 3.0;
    let sheet = long(&s);
    assert!((0.5..0.8).contains(&scrap), "scrap {scrap}");
    assert!((2.8..3.4).contains(&sheet), "sheet {sheet}");
}

#[test]
fn folds_stack_layers_and_shorten_the_sheet() {
    let mut s = Writing::roll_as(3, WritingKind::Note);
    s.note.length = 2.0;
    s.note.tear = 0.0;
    let mut dims = |form| {
        s.note.form = form;
        let (lo, hi) = bounds(&Writing::build(&s).parts);
        (hi[0] - lo[0], hi[2] - lo[2], hi[1] - lo[1])
    };
    let flat = dims(NoteForm::Flat);
    let once = dims(NoteForm::Folded);
    let twice = dims(NoteForm::FoldedTwice);
    let shortest = |d: (f32, f32, f32)| d.0.min(d.1);
    assert!(shortest(twice) < shortest(once) + 0.01 || once.0.max(once.1) < flat.0.max(flat.1), "{flat:?} {once:?} {twice:?}");
    assert!(once.2 > flat.2 * 0.5 && twice.2 > 0.02, "{flat:?} {once:?} {twice:?}");
}

#[test]
fn wear_tears_and_stains_a_note() {
    let mut s = Writing::roll_as(5, WritingKind::Note);
    s.note.form = NoteForm::Flat;
    s.note.tear = 0.0;
    let clean = Writing::build(&s);
    s.cond = Condition::of(ConditionState::GraveFind);
    let worn = Writing::build(&s);
    assert_ne!(part(&clean, "sheet").unwrap().positions, part(&worn, "sheet").unwrap().positions);
    assert_ne!(part(&clean, "sheet").unwrap().tint, part(&worn, "sheet").unwrap().tint);
    assert!(part(&clean, "foxing").is_none() && part(&worn, "foxing").is_some());
}
