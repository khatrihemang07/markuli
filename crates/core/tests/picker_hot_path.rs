//! Standards rule 8 for the Color Picker and Width Picker: dragging in the
//! saturation/value square, the hue strip or the slider, and hovering the
//! Presets, allocate nothing of our own per pointer move.
//!
//! Lives in its own test binary because it installs a counting global
//! allocator; it holds a single test so nothing else allocates concurrently.
//!
//! Known, accepted remainder: tiny-skia's rasterizer makes a few small
//! transient allocations per `fill_path` call (edge list and coverage runs,
//! a few KiB, freed immediately). They are bounded by the damaged region's
//! width, not by Ink or the screen, so the test caps bytes rather than
//! demanding zero calls.

use markuli_core::{Annotator, Button, DisplayId, Event, Format, PickerPart, Point};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use tiny_skia::Pixmap;

struct Counting;

static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

fn count(size: usize) {
    CALLS.fetch_add(1, Ordering::Relaxed);
    BYTES.fetch_add(size, Ordering::Relaxed);
}

// SAFETY: forwards every call unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn handle_and_render(a: &mut Annotator, pm: &mut Pixmap, at: Point) -> (usize, usize, usize) {
    let c0 = CALLS.load(Ordering::Relaxed);
    a.handle(Event::PointerMove(at));
    let (c1, b1) = (CALLS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
    let r0 = CALLS.load(Ordering::Relaxed);
    a.render(&mut pm.as_mut(), Format::Rgba);
    (
        c1 - c0,
        BYTES.load(Ordering::Relaxed) - b1,
        CALLS.load(Ordering::Relaxed) - r0,
    )
}

/// 40 moves along `along`, after a warm-up of the same path.
fn assert_alloc_free(what: &str, a: &mut Annotator, pm: &mut Pixmap, along: &dyn Fn(u16) -> Point) {
    for i in 0..20 {
        handle_and_render(a, pm, along(i));
    }
    let (mut calls, mut bytes, mut render_calls) = (0, 0, 0);
    for i in 0..40 {
        let (c, b, r) = handle_and_render(a, pm, along(i));
        calls += c;
        bytes += b;
        render_calls += r;
    }
    assert_eq!(calls, 0, "{what}: allocations handling 40 moves");
    // The popover and the Toolbar next to it are repainted whole: tiny-skia's
    // transient edge buffers (measured about 430 KiB at this width) are
    // bounded by the width of the area, not by Ink; the same cap as a full
    // redraw in `hot_path.rs`.
    // Mostly tiny-skia's transient buffers and the Toolbar's icons (measured
    // 290 to 570); a path built or an icon parsed per cell would add hundreds.
    assert!(
        render_calls / 40 < 800,
        "{what}: {} calls per render",
        render_calls / 40
    );
    assert!(
        bytes / 40 < 1024 * 1024,
        "{what}: {} bytes per render",
        bytes / 40
    );
}

fn lerp(a: Point, b: Point, i: u16) -> Point {
    let t = f32::from(i % 40) / 40.0;
    Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

#[test]
fn dragging_and_hovering_the_pickers_allocates_nothing_per_move() {
    let mut a = Annotator::new();
    let mut pm = Pixmap::new(1200, 800).expect("pixmap");
    a.handle(Event::Resize {
        width: 1200,
        height: 800,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a.render(&mut pm.as_mut(), Format::Rgba);

    let slot = a.button_center(Button::Color(2)).expect("toolbar");
    a.handle(Event::SecondaryClick(slot));
    a.render(&mut pm.as_mut(), Format::Rgba);
    let (cell_a, cell_b) = (
        a.picker_point(PickerPart::Preset(0)).expect("open"),
        a.picker_point(PickerPart::Preset(35)).expect("open"),
    );
    assert_alloc_free("hover Presets", &mut a, &mut pm, &|i| {
        lerp(cell_a, cell_b, i)
    });

    let tab = a.picker_point(PickerPart::Tab(1)).expect("open");
    a.handle(Event::PointerDown(tab));
    a.handle(Event::PointerUp(tab));
    a.render(&mut pm.as_mut(), Format::Rgba);

    let at = |a: &Annotator, part| a.picker_point(part).expect("open");
    let (s0, s1) = (
        at(&a, PickerPart::Square(0.05, 0.05)),
        at(&a, PickerPart::Square(0.95, 0.95)),
    );
    a.handle(Event::PointerDown(s0));
    assert_alloc_free("square drag", &mut a, &mut pm, &|i| lerp(s0, s1, i));
    let (h0, h1) = (at(&a, PickerPart::Hue(0.02)), at(&a, PickerPart::Hue(0.98)));
    a.handle(Event::PointerUp(s1));

    // A new drag, on the hue strip: every move refills the square.
    a.handle(Event::SecondaryClick(slot));
    let tab = at(&a, PickerPart::Tab(1));
    a.handle(Event::PointerDown(tab));
    a.handle(Event::PointerUp(tab));
    a.handle(Event::PointerDown(h0));
    assert_alloc_free("hue drag", &mut a, &mut pm, &|i| lerp(h0, h1, i));
    a.handle(Event::PointerUp(h1));

    let width = a.button_center(Button::Width(1)).expect("toolbar");
    a.handle(Event::SecondaryClick(width));
    a.render(&mut pm.as_mut(), Format::Rgba);
    let (t0, t1) = (
        at(&a, PickerPart::Track(0.0)),
        at(&a, PickerPart::Track(1.0)),
    );
    a.handle(Event::PointerDown(t0));
    assert_alloc_free("width drag", &mut a, &mut pm, &|i| lerp(t0, t1, i));
    a.handle(Event::PointerUp(t1));
}
