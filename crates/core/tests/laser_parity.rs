//! Seam 2: laser trail outline parity with `@excalidraw/laser-pointer` 1.3.1.
//!
//! `fixtures/laser.txt` is produced by `node tools/fixture-gen/gen.mjs laser`,
//! which runs the JS library with Excalidraw's trail options. Outlines must
//! match within 0.01 px, vertex for vertex.

use markuli_core::laser::get_trail;

struct Case {
    name: String,
    now: f64,
    close: bool,
    points: Vec<[f64; 3]>,
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
    for line in include_str!("fixtures/laser.txt").lines() {
        let mut parts = line.split(' ');
        let kind = parts.next();
        let nums: Vec<f64> = parts.clone().filter_map(|v| v.parse().ok()).collect();
        match kind {
            Some("case") => cases.push(Case {
                name: parts.next().expect("name").to_string(),
                now: field(line, "now"),
                close: field(line, "close") > 0.5,
                points: vec![],
                expected: vec![],
            }),
            Some("in") => cases
                .last_mut()
                .expect("case first")
                .points
                .push([nums[0], nums[1], nums[2]]),
            Some("out") => cases
                .last_mut()
                .expect("case first")
                .expected
                .push([nums[0], nums[1]]),
            _ => {}
        }
    }
    cases
}

#[test]
fn trails_match_laser_pointer_within_a_hundredth_of_a_pixel() {
    let cases = cases();
    assert!(cases.len() >= 15, "fixture has {} cases", cases.len());
    for case in cases {
        let got = get_trail(&case.points, case.now, case.close);
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
