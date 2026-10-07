//! Seam 1: copy text out. The JSON is parsed with a real parser (dev-only
//! dependency) and compared with the Excalidraw clipboard shape written down
//! in the research notes (`excalidraw-model.md` section 1 and 2), which come
//! from Excalidraw's `newFreeDrawElement` and `serializeAsClipboardJSON`.

use markuli_core::{Annotator, DisplayId, Event, Key, Point};
use serde_json::Value;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn session() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: 800,
        height: 600,
    });
    a.handle(Event::ToggleDrawMode(DisplayId(1)));
    a
}

fn key(a: &mut Annotator, c: char, command: bool) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command,
        shift: false,
    });
}

fn stroke(a: &mut Annotator, points: &[(f32, f32)]) {
    key(a, 'p', false);
    a.handle(Event::PointerDown(p(points[0].0, points[0].1)));
    for &(x, y) in &points[1..] {
        a.handle(Event::PointerMove(p(x, y)));
    }
    let (x, y) = points[points.len() - 1];
    a.handle(Event::PointerUp(p(x, y)));
}

fn copy(a: &mut Annotator) -> Option<String> {
    key(a, 'c', true);
    a.take_copy()
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("the copy text is valid JSON")
}

/// An `[x, y]` pair as numbers (JSON does not tell 110 from 110.0).
fn pair(v: &Value) -> (f64, f64) {
    let a = v.as_array().expect("a pair");
    assert_eq!(a.len(), 2);
    (a[0].as_f64().expect("x"), a[1].as_f64().expect("y"))
}

fn elements(doc: &Value) -> &Vec<Value> {
    doc["elements"].as_array().expect("elements is an array")
}

/// Every field Excalidraw's `newFreeDrawElement` writes, in its JSON order,
/// with the kind of value it holds.
const FIELDS: [(&str, &str); 30] = [
    ("id", "string"),
    ("type", "string"),
    ("x", "number"),
    ("y", "number"),
    ("width", "number"),
    ("height", "number"),
    ("angle", "number"),
    ("strokeColor", "string"),
    ("backgroundColor", "string"),
    ("fillStyle", "string"),
    ("strokeWidth", "number"),
    ("strokeStyle", "string"),
    ("roughness", "number"),
    ("opacity", "number"),
    ("groupIds", "array"),
    ("frameId", "null"),
    ("index", "string"),
    ("roundness", "null"),
    ("seed", "integer"),
    ("version", "integer"),
    ("versionNonce", "integer"),
    ("isDeleted", "boolean"),
    ("boundElements", "null"),
    ("updated", "integer"),
    ("link", "null"),
    ("locked", "boolean"),
    ("points", "array"),
    ("pressures", "array"),
    ("simulatePressure", "boolean"),
    ("lastCommittedPoint", "array"),
];

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[test]
fn nothing_to_copy_without_ink_or_outside_draw_mode() {
    let mut a = session();
    assert_eq!(copy(&mut a), None);
    stroke(&mut a, &[(100.0, 100.0), (200.0, 150.0)]);
    a.handle(Event::ToggleDrawMode(DisplayId(1)));
    key(&mut a, 'c', true);
    assert_eq!(a.take_copy(), None);
}

#[test]
fn the_copy_text_is_taken_once() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (200.0, 150.0)]);
    assert!(copy(&mut a).is_some());
    assert_eq!(a.take_copy(), None);
}

#[test]
fn the_wrapper_is_an_excalidraw_clipboard_document() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (200.0, 150.0)]);
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    let object = doc.as_object().expect("an object");
    let mut keys: Vec<_> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["elements", "files", "type"]);
    assert_eq!(doc["type"], "excalidraw/clipboard");
    assert_eq!(doc["files"], serde_json::json!({}));
}

#[test]
fn a_stroke_has_exactly_the_freedraw_field_set_with_the_right_types() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (150.0, 130.0), (210.0, 120.5)]);
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    let element = &elements(&doc)[0];
    let object = element.as_object().expect("an object");
    assert_eq!(object.len(), FIELDS.len(), "no extra or missing fields");
    for (name, wanted) in FIELDS {
        let value = object.get(name).unwrap_or_else(|| panic!("{name} missing"));
        let got = kind(value);
        let ok = got == wanted || (wanted == "number" && got == "integer");
        assert!(ok, "{name}: {got}, wanted {wanted}");
    }
    // Excalidraw writes them in this order; keep it so diffs read the same.
    // (The parsed map is sorted, so read the order from the text.)
    let text = copy(&mut a).expect("ink to copy");
    let text = &text[text.find("\"elements\":[").expect("elements")..];
    let at: Vec<_> = FIELDS
        .iter()
        .map(|(n, _)| text.find(&format!("\"{n}\":")).unwrap_or(usize::MAX))
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{at:?}");
}

#[test]
fn a_stroke_carries_its_geometry_and_style() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (150.0, 130.0), (210.0, 120.5)]);
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    let e = &elements(&doc)[0];
    assert_eq!(e["type"], "freedraw");
    assert_eq!((e["x"].as_f64(), e["y"].as_f64()), (Some(100.0), Some(100.0)));
    assert_eq!(pair(&e["points"][0]), (0.0, 0.0));
    // The pointer-up point is kept even when it repeats the last move.
    assert_eq!(e["points"].as_array().map(Vec::len), Some(4));
    assert_eq!(pair(&e["points"][3]), (110.0, 20.5));
    assert_eq!(pair(&e["lastCommittedPoint"]), (110.0, 20.5));
    assert_eq!(e["width"].as_f64(), Some(110.0));
    assert_eq!(e["height"].as_f64(), Some(30.0));
    assert_eq!(e["strokeColor"], "#e03131");
    assert_eq!(e["backgroundColor"], "transparent");
    assert_eq!(e["strokeWidth"].as_f64(), Some(2.0));
    assert_eq!(e["opacity"], 100);
    assert_eq!(e["angle"].as_f64(), Some(0.0));
    assert_eq!(e["roughness"], 1);
    assert_eq!(e["fillStyle"], "solid");
    assert_eq!(e["strokeStyle"], "solid");
    assert_eq!(e["isDeleted"], false);
    assert_eq!(e["locked"], false);
    assert!(e["version"].as_u64() >= Some(1));
}

#[test]
fn a_mouse_stroke_simulates_pressure_and_stores_none() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (200.0, 150.0)]);
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    let e = &elements(&doc)[0];
    assert_eq!(e["simulatePressure"], true);
    assert_eq!(e["pressures"], serde_json::json!([]));
}

#[test]
fn a_pen_stroke_stores_one_pressure_per_point() {
    let mut a = session();
    a.handle(Event::Pressure(Some(0.25)));
    stroke(&mut a, &[(100.0, 100.0), (150.0, 120.0), (200.0, 150.0)]);
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    let e = &elements(&doc)[0];
    assert_eq!(e["simulatePressure"], false);
    let pressures = e["pressures"].as_array().expect("array");
    assert_eq!(pressures.len(), e["points"].as_array().expect("array").len());
    assert!(pressures.iter().all(|v| v.as_f64() == Some(0.25)));
}

#[test]
fn ids_are_unique_and_indices_ascend_in_z_order() {
    let mut a = session();
    for y in [100.0, 200.0, 300.0] {
        stroke(&mut a, &[(100.0, y), (200.0, y + 20.0)]);
    }
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    let list = elements(&doc);
    assert_eq!(list.len(), 3);
    let ids: Vec<_> = list.iter().map(|e| e["id"].as_str().expect("id")).collect();
    assert!(ids.iter().all(|id| !id.is_empty()));
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
    let index: Vec<_> = list
        .iter()
        .map(|e| e["index"].as_str().expect("index"))
        .collect();
    assert!(index.windows(2).all(|w| w[0] < w[1]), "{index:?}");
    let ys: Vec<_> = list.iter().map(|e| e["y"].as_f64()).collect();
    assert_eq!(ys, [Some(100.0), Some(200.0), Some(300.0)], "z-order kept");
}

#[test]
fn with_a_selection_only_the_selected_elements_are_copied() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (200.0, 100.0)]);
    stroke(&mut a, &[(100.0, 300.0), (200.0, 300.0)]);
    key(&mut a, 'v', false);
    a.handle(Event::PointerDown(p(150.0, 300.0)));
    a.handle(Event::PointerUp(p(150.0, 300.0)));
    let doc = parse(&copy(&mut a).expect("a selection to copy"));
    assert_eq!(elements(&doc).len(), 1);
    assert_eq!(elements(&doc)[0]["y"].as_f64(), Some(300.0));
    assert_eq!(a.selection().len(), 1, "copying keeps the selection");
}

#[test]
fn a_moved_element_is_copied_at_its_new_position() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (200.0, 100.0)]);
    key(&mut a, 'v', false);
    a.handle(Event::PointerDown(p(150.0, 100.0)));
    a.handle(Event::PointerMove(p(170.0, 130.0)));
    a.handle(Event::PointerUp(p(170.0, 130.0)));
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    let e = &elements(&doc)[0];
    assert_eq!((e["x"].as_f64(), e["y"].as_f64()), (Some(120.0), Some(130.0)));
}

#[test]
fn a_deleted_element_is_not_copied() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (200.0, 100.0)]);
    stroke(&mut a, &[(100.0, 300.0), (200.0, 300.0)]);
    key(&mut a, 'v', false);
    a.handle(Event::PointerDown(p(150.0, 100.0)));
    a.handle(Event::PointerUp(p(150.0, 100.0)));
    a.handle(Event::Key {
        key: Key::Delete,
        command: false,
        shift: false,
    });
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    assert_eq!(elements(&doc).len(), 1);
    assert_eq!(elements(&doc)[0]["y"].as_f64(), Some(300.0));
}

#[test]
fn tiny_and_huge_numbers_stay_valid_json() {
    let mut a = session();
    // A dot: one point nudged by 0.0001, a very small width.
    a.handle(Event::PointerDown(p(300.0, 300.0)));
    a.handle(Event::PointerUp(p(300.0, 300.0)));
    let doc = parse(&copy(&mut a).expect("ink to copy"));
    let e = &elements(&doc)[0];
    assert!(e["width"].as_f64().is_some_and(|w| w > 0.0 && w < 0.001));
}
