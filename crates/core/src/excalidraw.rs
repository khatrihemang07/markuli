//! Excalidraw clipboard JSON, written by hand (no serde, standards rule 9).
//!
//! Source: `serializeAsClipboardJSON` (packages/excalidraw/clipboard.ts) wraps
//! `{ type: "excalidraw/clipboard", elements, files }`, and `newFreeDrawElement`
//! (element/newElement.ts) fixes each element's fields. Excalidraw 0.18, MIT.
//! The fields are written in the order `JSON.stringify` produces them there.
//! Paste goes through `parseClipboard` and `restoreElements`
//! (data/restore.ts), which accept this shape and fill nothing in.
//!
//! Deviations: `updated` is 0 (the core has no clock; paste only uses it for
//! collaboration), `versionNonce` repeats the seed, and `files` is `{}`.

use crate::Element;
use std::fmt::Write;

/// The clipboard text for `elements` in z-order, or `None` when empty.
pub(crate) fn clipboard<'a>(elements: impl Iterator<Item = &'a Element>) -> Option<String> {
    let mut out = String::from(r#"{"type":"excalidraw/clipboard","elements":["#);
    let mut count = 0_usize;
    for element in elements {
        if count > 0 {
            out.push(',');
        }
        write_element(&mut out, element, count);
        count += 1;
    }
    out.push_str(r#"],"files":{}}"#);
    (count > 0).then_some(out)
}

fn write_element(out: &mut String, e: &Element, z: usize) {
    let [l, t, r, b] = e.extent();
    let [red, green, blue] = e.stroke_color();
    // Writing to a String cannot fail.
    let _ = write!(
        out,
        r##"{{"id":"markuli-{id}","type":"freedraw","x":{x},"y":{y},"width":{w},"height":{h},"angle":0,"strokeColor":"#{red:02x}{green:02x}{blue:02x}","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":{sw},"strokeStyle":"solid","roughness":1,"opacity":{op},"groupIds":[],"frameId":null,"index":{index},"roundness":null,"seed":{seed},"version":{version},"versionNonce":{seed},"isDeleted":false,"boundElements":null,"updated":0,"link":null,"locked":false,"points":["##,
        id = e.id(),
        x = Num(e.x()),
        y = Num(e.y()),
        w = Num(r - l),
        h = Num(b - t),
        sw = Num(e.stroke_width()),
        op = e.opacity(),
        index = Index(z),
        seed = e.seed(),
        version = e.version(),
    );
    for (i, p) in e.points().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "[{},{}]", Num(p.x), Num(p.y));
    }
    out.push_str(r#"],"pressures":["#);
    for (i, pressure) in e.pressures().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{}", Num(*pressure));
    }
    let _ = write!(
        out,
        r#"],"simulatePressure":{},"lastCommittedPoint":"#,
        e.simulate_pressure()
    );
    match e.points().last() {
        Some(p) => {
            let _ = write!(out, "[{},{}]", Num(p.x), Num(p.y));
        }
        None => out.push_str("null"),
    }
    out.push('}');
}

/// A JSON number. Rust prints the shortest text that reads back as the same
/// `f32`, never with an exponent; NaN and infinity (which JSON lacks) become 0.
struct Num(f32);

impl std::fmt::Display for Num {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_finite() {
            write!(f, "{}", self.0)
        } else {
            f.write_str("0")
        }
    }
}

/// A fractional index (the `index` field), as JSON: strictly ascending with
/// `z`. Excalidraw's own keys start `a0`..`az` (one digit, base 62) and then
/// `b00`..`bzz`; past that it is `null`, which paste re-numbers.
struct Index(usize);

impl std::fmt::Display for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let digit = |n: usize| char::from(DIGITS[n % 62]);
        match self.0 {
            z if z < 62 => write!(f, "\"a{}\"", digit(z)),
            z if z < 62 + 62 * 62 => {
                let n = z - 62;
                write!(f, "\"b{}{}\"", digit(n / 62), digit(n))
            }
            _ => f.write_str("null"),
        }
    }
}
