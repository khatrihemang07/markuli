//! Seam 1: the Toolbar Position (Top, Bottom, Left, Right), with insets on all
//! four sides, through events and pixels.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    reason = "test pixel coordinates are small and positive"
)]

use markuli_core::{
    Annotator, Button, DisplayId, Event, Format, Insets, Point, ToolbarPosition as Pos,
};
use tiny_skia::Pixmap;

const W: u32 = 1600;
const H: u32 = 1000;

fn insets(scale: f32) -> Insets {
    let s = |v: f32| (v * scale) as u32;
    Insets {
        top: s(37.0),
        bottom: s(70.0),
        left: s(60.0),
        right: s(48.0),
    }
}

fn scene(pos: Pos, scale: f32, width: u32, height: u32) -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::Resize { width, height });
    a.handle(Event::Insets(insets(scale)));
    a.handle(Event::ToolbarPosition(pos));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a
}

fn render(a: &mut Annotator, pm: &mut Pixmap) {
    a.render(&mut pm.as_mut(), Format::Rgba);
}

fn centers(a: &Annotator) -> Vec<Point> {
    let mut all = vec![];
    all.extend((0..4).map(|i| a.button_center(Button::Tool(i)).expect("tool")));
    all.extend((0..5).map(|i| a.button_center(Button::Color(i)).expect("color")));
    all.extend((0..3).map(|i| a.button_center(Button::Width(i)).expect("width")));
    all.extend(
        [Button::Undo, Button::Redo, Button::Clear].map(|b| a.button_center(b).expect("action")),
    );
    all
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.5
}

#[test]
fn row_and_column_shapes_are_centred_between_the_insets() {
    for scale in [1.0_f32, 2.0] {
        let (w, h) = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
        let i = insets(scale);
        let (l, t, r, b) = (
            i.left as f32,
            i.top as f32,
            (w - i.right) as f32,
            (h - i.bottom) as f32,
        );
        for pos in Pos::ALL {
            let a = scene(pos, scale, w, h);
            let c = centers(&a);
            let (first, last) = (c[0], c[c.len() - 1]);
            match pos {
                Pos::Top | Pos::Bottom => {
                    assert!(c.iter().all(|p| near(p.y, first.y)), "{pos:?}: one row");
                    assert!(
                        c.windows(2).all(|p| p[0].x < p[1].x),
                        "{pos:?}: left to right"
                    );
                    assert!(
                        near(first.x.midpoint(last.x), l.midpoint(r)),
                        "{pos:?} {scale}"
                    );
                    // Margin 16 (24 at the bottom) + 4 padding + 16 half button.
                    let (edge, want) = if pos == Pos::Top {
                        (first.y - t, 36.0)
                    } else {
                        (b - first.y, 44.0)
                    };
                    assert!(near(edge, want * scale), "{pos:?}: edge {edge}");
                }
                Pos::Left | Pos::Right => {
                    assert!(c.iter().all(|p| near(p.x, first.x)), "{pos:?}: one column");
                    assert!(
                        c.windows(2).all(|p| p[0].y < p[1].y),
                        "{pos:?}: top to bottom"
                    );
                    assert!(
                        near(first.y.midpoint(last.y), t.midpoint(b)),
                        "{pos:?} {scale}"
                    );
                    let edge = if pos == Pos::Left {
                        first.x - l
                    } else {
                        r - first.x
                    };
                    assert!(near(edge, 36.0 * scale), "{pos:?}: edge {edge}");
                }
            }
        }
    }
}

#[test]
fn no_button_or_shadow_reaches_into_an_inset() {
    for scale in [1.0_f32, 2.0] {
        let (w, h) = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
        let i = insets(scale);
        for pos in Pos::ALL {
            let mut a = scene(pos, scale, w, h);
            let half = 16.0 * scale;
            for p in centers(&a) {
                assert!(p.x - half >= i.left as f32 && p.x + half <= (w - i.right) as f32);
                assert!(p.y - half >= i.top as f32 && p.y + half <= (h - i.bottom) as f32);
            }
            let mut pm = Pixmap::new(w, h).expect("pixmap");
            render(&mut a, &mut pm);
            for y in 0..h {
                for x in 0..w {
                    let inside = x >= i.left && x < w - i.right && y >= i.top && y < h - i.bottom;
                    let alpha = pm.data()[((y * w + x) * 4 + 3) as usize];
                    assert!(inside || alpha <= 1, "{pos:?} {scale}: pixel {x},{y}");
                }
            }
        }
    }
}

#[test]
fn a_column_that_does_not_fit_falls_back_to_the_top_row() {
    // 560 high, 107 of it insets: the 612 px column cannot fit.
    for pos in [Pos::Left, Pos::Right] {
        let a = scene(pos, 1.0, W, 560);
        let c = centers(&a);
        assert!(c.iter().all(|p| near(p.y, c[0].y)), "{pos:?}: a row");
        assert!(near(c[0].y, 37.0 + 36.0), "{pos:?}: at the top");
    }
}

#[test]
fn a_column_needs_its_length_plus_both_margins() {
    // 616 px of buttons + 2 x 16 margin + 107 of insets = 755.
    let fits = scene(Pos::Left, 1.0, W, 756);
    assert!(near(centers(&fits)[0].x, 60.0 + 36.0));
    let tight = scene(Pos::Left, 1.0, W, 754);
    assert!(near(centers(&tight)[0].y, 37.0 + 36.0));
}

#[test]
fn a_new_position_asks_for_a_render_and_leaves_no_old_pixels() {
    for scale in [1.0_f32, 2.0] {
        let (w, h) = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
        let mut a = scene(Pos::Top, scale, w, h);
        let mut live = Pixmap::new(w, h).expect("pixmap");
        render(&mut a, &mut live);
        for pos in [Pos::Left, Pos::Bottom, Pos::Right, Pos::Top] {
            assert!(!a.view().needs_render);
            let view = a.handle(Event::ToolbarPosition(pos));
            assert!(view.needs_render, "{pos:?}");
            render(&mut a, &mut live);
            let mut fresh = Pixmap::new(w, h).expect("pixmap");
            render(&mut scene(pos, scale, w, h), &mut fresh);
            assert!(live.data() == fresh.data(), "{pos:?} {scale}: leftovers");
        }
    }
}

#[test]
fn changed_insets_repaint_and_leave_no_old_pixels() {
    let mut a = scene(Pos::Left, 1.0, W, H);
    let mut live = Pixmap::new(W, H).expect("pixmap");
    render(&mut a, &mut live);
    let moved = Insets {
        top: 0,
        bottom: 0,
        left: 200,
        right: 0,
    };
    assert!(a.handle(Event::Insets(moved)).needs_render);
    render(&mut a, &mut live);
    let mut b = scene(Pos::Left, 1.0, W, H);
    b.handle(Event::Insets(moved));
    let mut fresh = Pixmap::new(W, H).expect("pixmap");
    render(&mut b, &mut fresh);
    assert!(live.data() == fresh.data(), "leftover pixels");
}

#[test]
fn a_click_on_a_button_works_in_every_position() {
    for pos in Pos::ALL {
        let mut a = scene(pos, 1.0, W, H);
        let eraser = a.button_center(Button::Tool(2)).expect("eraser");
        a.handle(Event::PointerDown(eraser));
        let view = a.handle(Event::PointerUp(eraser));
        assert_eq!(view.tool, markuli_core::ToolKind::Eraser, "{pos:?}");
        assert_eq!(a.ink().len(), 0, "{pos:?}");
    }
}

#[test]
fn all_lists_each_position_once_starting_with_the_default() {
    let all = Pos::ALL;
    assert_eq!(all[0], Pos::default());
    for (i, a) in all.iter().enumerate() {
        assert!(all[i + 1..].iter().all(|b| a != b));
    }
}
