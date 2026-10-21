//! Seam 1: the Width Picker through the annotator interface only.

mod support;

use markuli_core::{Annotator, Button, Event, Palette, PickerPart, Style, ToolbarPosition};
use support::picker::{
    click, click_part, drag, escape, is_open, last, open, p, part, session, sized, stroke,
};

fn width_of(a: &Annotator, slot: usize) -> f32 {
    a.view().palette.widths[slot]
}

#[test]
fn a_secondary_click_on_a_width_slot_opens_the_width_picker() {
    let mut a = session();
    open(&mut a, Button::Width(1));
    assert!(is_open(&a));
    assert!(a.picker_point(PickerPart::Track(0.5)).is_some());
    assert!(a.picker_point(PickerPart::Preset(0)).is_none());
    assert!(a.view().edit.is_none(), "no native editor any more");
}

#[test]
fn the_track_runs_from_half_a_point_to_thirty() {
    let mut a = session();
    open(&mut a, Button::Width(1));
    drag(&mut a, &[PickerPart::Track(0.0)]);
    assert!((width_of(&a, 1) - 0.5).abs() < f32::EPSILON);
    open(&mut a, Button::Width(1));
    drag(&mut a, &[PickerPart::Track(1.0)]);
    assert!((width_of(&a, 1) - 30.0).abs() < f32::EPSILON);
}

#[test]
fn the_width_snaps_to_a_quarter_point() {
    let mut a = session();
    open(&mut a, Button::Width(1));
    for f in [0.13, 0.27, 0.5, 0.731, 0.99] {
        a.handle(Event::PointerDown(part(&a, PickerPart::Track(f))));
        let w = width_of(&a, 1);
        assert_eq!(Palette::snap_width(w), Some(w), "{f}: {w}");
        a.handle(Event::PointerUp(part(&a, PickerPart::Track(f))));
        open(&mut a, Button::Width(1));
    }
}

#[test]
fn a_drag_is_live_and_its_release_closes() {
    let mut a = session();
    open(&mut a, Button::Width(2));
    a.handle(Event::PointerDown(part(&a, PickerPart::Track(0.2))));
    let first = width_of(&a, 2);
    a.handle(Event::PointerMove(part(&a, PickerPart::Track(0.8))));
    assert!(width_of(&a, 2) > first);
    assert_eq!(a.view().style, Style { color: 0, width: 2 });
    assert!(is_open(&a));
    a.handle(Event::PointerUp(part(&a, PickerPart::Track(0.8))));
    assert!(!is_open(&a));
    assert!(a.ink().is_empty());
    stroke(&mut a, 100.0);
    assert!((last(&a).stroke_width() - width_of(&a, 2)).abs() < f32::EPSILON);
}

#[test]
fn a_drag_past_the_ends_clamps() {
    let mut a = session();
    open(&mut a, Button::Width(0));
    a.handle(Event::PointerDown(part(&a, PickerPart::Track(0.5))));
    a.handle(Event::PointerMove(p(5000.0, 80.0)));
    assert!((width_of(&a, 0) - 30.0).abs() < f32::EPSILON);
    a.handle(Event::PointerMove(p(-5000.0, 80.0)));
    assert!((width_of(&a, 0) - 0.5).abs() < f32::EPSILON);
    a.handle(Event::PointerUp(p(-5000.0, 80.0)));
}

#[test]
fn reset_restores_the_default_and_closes() {
    let mut a = session();
    open(&mut a, Button::Width(2));
    drag(&mut a, &[PickerPart::Track(0.9)]);
    open(&mut a, Button::Width(2));
    click_part(&mut a, PickerPart::Reset);
    assert!((width_of(&a, 2) - Palette::DEFAULT.widths[2]).abs() < f32::EPSILON);
    assert!(!is_open(&a));
}

#[test]
fn escape_and_an_outside_click_close_it() {
    let mut a = session();
    open(&mut a, Button::Width(0));
    escape(&mut a);
    assert!(!is_open(&a));
    open(&mut a, Button::Width(0));
    a.handle(Event::PointerDown(p(300.0, 500.0)));
    a.handle(Event::PointerUp(p(300.0, 500.0)));
    assert!(!is_open(&a));
    assert!(a.ink().is_empty());
}

#[test]
fn a_click_on_the_chosen_width_slot_opens_it() {
    let mut a = session();
    let at = support::picker::center(&a, Button::Width(1));
    click(&mut a, at);
    assert!(is_open(&a));
}

#[test]
fn a_width_picker_stays_on_the_screen_on_every_side() {
    for position in ToolbarPosition::ALL {
        let mut a = sized(900, 700, position);
        open(&mut a, Button::Width(2));
        for f in [0.0, 1.0] {
            let at = part(&a, PickerPart::Track(f));
            assert!(at.x > 0.0 && at.x < 900.0 && at.y > 0.0 && at.y < 700.0);
        }
        let reset = part(&a, PickerPart::Reset);
        assert!(reset.x > 0.0 && reset.x < 900.0 && reset.y > 0.0 && reset.y < 700.0);
    }
}
