//! Seam 1: Recent Colors through the annotator interface only.

mod support;

use markuli_core::{Annotator, Button, Event, PickerPart, RecentColors};
use support::picker::{click_part, drag, open, session};

fn recent(a: &Annotator) -> Vec<[u8; 3]> {
    a.view().recent.as_slice().to_vec()
}

fn pick_preset(a: &mut Annotator, cell: usize) {
    open(a, Button::Color(0));
    click_part(a, PickerPart::Preset(cell));
}

#[test]
fn it_starts_empty() {
    assert_eq!(session().view().recent, RecentColors::default());
}

#[test]
fn the_startup_list_is_reported() {
    let mut a = session();
    let list = RecentColors::new(&[[1, 2, 3], [4, 5, 6]]);
    a.handle(Event::RecentColors(list));
    assert_eq!(a.view().recent, list);
    assert_eq!(recent(&a), vec![[1, 2, 3], [4, 5, 6]]);
}

#[test]
fn the_startup_list_is_capped_at_six() {
    let many: Vec<[u8; 3]> = (0..9).map(|i| [i, 0, 0]).collect();
    assert_eq!(RecentColors::new(&many).as_slice().len(), 6);
    assert_eq!(RecentColors::new(&many).as_slice()[0], [0, 0, 0]);
}

#[test]
fn a_pick_goes_to_the_front() {
    let mut a = session();
    pick_preset(&mut a, 0);
    pick_preset(&mut a, 9);
    assert_eq!(recent(&a), vec![[0xce, 0xd4, 0xda], [0xe9, 0xec, 0xef]]);
}

#[test]
fn a_repeated_color_moves_to_the_front_without_a_duplicate() {
    let mut a = session();
    pick_preset(&mut a, 0);
    pick_preset(&mut a, 9);
    pick_preset(&mut a, 0);
    assert_eq!(recent(&a), vec![[0xe9, 0xec, 0xef], [0xce, 0xd4, 0xda]]);
}

#[test]
fn only_the_last_six_are_kept() {
    let mut a = session();
    let mut picked = Vec::new();
    for cell in 0..8 {
        pick_preset(&mut a, cell);
        picked.push(a.view().palette.colors[0]);
    }
    picked.reverse();
    picked.truncate(6);
    assert_eq!(recent(&a), picked);
}

#[test]
fn a_recent_cell_picks_its_color_and_moves_it_to_the_front() {
    let mut a = session();
    pick_preset(&mut a, 0);
    pick_preset(&mut a, 9);
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Recent(1));
    assert_eq!(a.view().palette.colors[2], [0xe9, 0xec, 0xef]);
    assert_eq!(recent(&a)[0], [0xe9, 0xec, 0xef]);
    assert!(!support::picker::is_open(&a));
}

#[test]
fn a_custom_drag_adds_one_color_when_it_ends() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    drag(
        &mut a,
        &[
            PickerPart::Square(0.2, 0.2),
            PickerPart::Square(0.5, 0.4),
            PickerPart::Square(0.8, 0.6),
        ],
    );
    let list = recent(&a);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0], a.view().palette.colors[2]);
}

#[test]
fn a_system_panel_run_adds_its_last_color_at_edit_end() {
    let mut a = session();
    a.handle(Event::EditColor {
        slot: 1,
        rgb: [10, 20, 30],
    });
    a.handle(Event::EditColor {
        slot: 1,
        rgb: [40, 50, 60],
    });
    assert_eq!(a.view().recent, RecentColors::default());
    a.handle(Event::EditEnd);
    assert_eq!(recent(&a), vec![[40, 50, 60]]);
    // A second EditEnd with no run adds nothing.
    a.handle(Event::EditEnd);
    assert_eq!(recent(&a).len(), 1);
}
