//! Writing "Forge new": deterministic, in range, varied and coherent.

use armory::families::Generator;
use armory::families::writing::{Cover, Edge, INKS,Knob, LIMITS, Paper, SEALS, Tie, Writing, WritingKind, WritingState};

fn rolls(range: std::ops::RangeInclusive<u32>) -> Vec<WritingState> {
    range.map(Writing::roll).collect()
}

#[test]
fn same_seed_same_writing() {
    for seed in [0, 1, 7, 1207, 99_999, u32::MAX] {
        assert_eq!(Writing::roll(seed), Writing::roll(seed));
        assert_eq!(Writing::roll(seed).seed, seed);
    }
    assert_ne!(Writing::roll(1), Writing::roll(2));
}

#[test]
fn state_round_trips_through_json() {
    let s = Writing::roll(1207);
    let back: WritingState = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, back);
}

#[test]
fn roll_as_matches_roll() {
    for seed in [3, 42, 77] {
        let s = Writing::roll(seed);
        assert_eq!(Writing::roll_as(seed, s.piece.kind), s, "seed {seed}");
    }
}

#[test]
fn every_slider_within_range() {
    for s in rolls(1..=1024) {
        let bad = s.out_of_range(&LIMITS);
        assert!(bad.is_empty(), "seed {}: {bad:?}", s.seed);
    }
}

#[test]
fn rolls_are_coherent() {
    for s in rolls(1..=1024) {
        let sc = &s.scroll;
        if !sc.is_shut() {
            assert!(sc.tie == Tie::None && !sc.seal, "seed {}: an open scroll is tied or sealed", s.seed);
        }
        assert!(s.piece.paper.colors().contains(&s.piece.color), "seed {}: colour is not the paper's", s.seed);
    }
}

#[test]
fn first_256_seeds_cover_the_variety() {
    let all = rolls(1..=256);
    let any = |f: &dyn Fn(&WritingState) -> bool| all.iter().any(f);
    for kind in WritingKind::ALL {
        assert!(any(&|s| s.piece.kind == kind), "no {kind:?}");
    }
    let scroll = |f: &dyn Fn(&WritingState) -> bool| any(&|s| s.piece.kind == WritingKind::Scroll && f(s));
    for paper in Paper::ALL {
        assert!(scroll(&|s| s.piece.paper == paper), "no {paper:?} scroll");
    }
    for n in 0..=2 {
        assert!(scroll(&|s| s.scroll.rollers == n), "no scroll with {n} rollers");
    }
    for knob in Knob::ALL {
        assert!(scroll(&|s| s.scroll.rollers > 0 && s.scroll.knob == knob), "no {knob:?} knobs");
    }
    for tie in Tie::ALL {
        assert!(scroll(&|s| s.scroll.tie == tie && s.scroll.is_shut()), "no {tie:?} tie");
    }
    for wax in SEALS {
        assert!(scroll(&|s| s.scroll.seal && s.scroll.seal_color == wax), "no {wax:?} seal");
    }
    for ink in INKS {
        assert!(scroll(&|s| s.piece.ink == ink), "no {ink:?} ink");
    }
    assert!(scroll(&|s| s.scroll.is_shut()) && scroll(&|s| !s.scroll.is_shut()));
    assert!(scroll(&|s| s.scroll.seal && s.scroll.tie != Tie::None));
    assert!(scroll(&|s| s.scroll.seal && s.scroll.tie == Tie::None));
}

#[test]
fn nouns() {
    let mut s = WritingState::default();
    assert_eq!(Writing::noun(&s), "Scroll");
    s.piece.paper = Paper::Papyrus;
    assert_eq!(Writing::noun(&s), "Papyrus Scroll");
    s.piece.paper = Paper::Vellum;
    assert_eq!(Writing::noun(&s), "Vellum Scroll");
    s.scroll.seal = true;
    assert_eq!(Writing::noun(&s), "Sealed Scroll");
    s.scroll.unroll = 0.5;
    assert_eq!(Writing::noun(&s), "Vellum Scroll");
    s.piece.kind = WritingKind::Note;
    assert_eq!(Writing::noun(&s), "Note");
    s.piece.kind = WritingKind::Book;
    assert_eq!(Writing::noun(&s), "Book");
}

/// Saved items depend on the draw order: edits to the roll must not move these.
#[test]
fn golden_rolls_stay_put() {
    use armory::families::writing::NoteForm;
    let near = |a: f32, b: f32| assert!((a - b).abs() < 1e-4, "{a} != {b}");

    let s = Writing::roll_as(1207, WritingKind::Scroll);
    assert_eq!((s.piece.paper, s.piece.lines), (Paper::Parchment, 9));
    assert_eq!((s.scroll.rollers, s.scroll.knob, s.scroll.sigil, s.scroll.tie, s.scroll.seal), (1, Knob::Bone, 6, Tie::None, false));
    near(s.piece.size, 1.13);
    near(s.scroll.turns, 6.2);
    near(s.scroll.unroll, 0.7);
    near(s.scroll.curl, 0.35);

    let s = Writing::roll_as(42, WritingKind::Note);
    assert_eq!((s.piece.paper, s.piece.lines, s.note.form, s.note.scrawl), (Paper::Vellum, 5, NoteForm::Sealed, 5));
    near(s.note.length, 1.1);
    near(s.note.aspect, 0.68);

    let s = Writing::roll_as(42, WritingKind::Book);
    assert_eq!((s.book.cover, s.book.bands, s.book.edge, s.book.ribbon), (Cover::Leather, 4, Edge::Red, true));
    near(s.book.length, 3.45);
    near(s.book.thick, 0.57);
}
