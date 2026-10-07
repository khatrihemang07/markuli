//! Seam 2: stroke-outline parity with the original perfect-freehand 1.2.0.
//!
//! `fixtures/freehand.txt` is produced by `tools/fixture-gen/gen.mjs`, which
//! runs the JS library with Excalidraw's freedraw options. Outlines must match
//! within 0.01 px.

use markuli_core::freehand::get_stroke;

struct Case {
    name: String,
    size: f64,
    last: bool,
    points: Vec<[f64; 2]>,
    pressures: Vec<f64>,
    expected: Vec<[f64; 2]>,
}

fn field(header: &str, key: &str) -> f64 {
    let prefix = format!("{key}=");
    header
        .split(' ')
        .find_map(|part| part.strip_prefix(&prefix))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("fixture header lacks {key}: {header}"))
}

fn cases() -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for line in include_str!("fixtures/freehand.txt").lines() {
        let mut parts = line.split(' ');
        let num = |s: Option<&str>| -> f64 { s.and_then(|v| v.parse().ok()).expect("number") };
        match parts.next() {
            Some("case") => {
                cases.push(Case {
                    name: parts.next().expect("name").to_string(),
                    size: field(line, "size"),
                    last: field(line, "last") > 0.5,
                    points: vec![],
                    pressures: vec![],
                    expected: vec![],
                });
            }
            Some("in") => {
                let case = cases.last_mut().expect("case first");
                let (x, y) = (num(parts.next()), num(parts.next()));
                case.points.push([x, y]);
                if let Some(p) = parts.next() {
                    case.pressures.push(num(Some(p)));
                }
            }
            Some("out") => {
                let case = cases.last_mut().expect("case first");
                let (x, y) = (num(parts.next()), num(parts.next()));
                case.expected.push([x, y]);
            }
            _ => {}
        }
    }
    cases
}

#[test]
fn outlines_match_perfect_freehand_within_a_hundredth_of_a_pixel() {
    let cases = cases();
    assert!(cases.len() >= 15, "fixture has {} cases", cases.len());
    for case in cases {
        let got = get_stroke(&case.points, &case.pressures, case.size, case.last);
        assert_eq!(
            got.len(),
            case.expected.len(),
            "{}: outline vertex count",
            case.name
        );
        for (i, (g, e)) in got.iter().zip(&case.expected).enumerate() {
            assert!(
                (g[0] - e[0]).abs() <= 0.01 && (g[1] - e[1]).abs() <= 0.01,
                "{} vertex {i}: got {g:?}, expected {e:?}",
                case.name
            );
        }
    }
}
