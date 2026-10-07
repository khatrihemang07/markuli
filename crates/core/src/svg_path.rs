//! A small SVG path-data parser, just enough for Excalidraw's icons
//! (`M L H V C S Q T A Z`, absolute and relative, implicit repeats).
//!
//! Own code instead of a crate (standards rule 9): about 150 lines, no
//! dependency tree. Arcs become cubic Beziers following the endpoint to
//! centre conversion of the SVG 1.1 implementation notes, appendix F.6.

use tiny_skia::{Path, PathBuilder};

type Pt = (f32, f32);

/// Parses `d`; `None` on malformed data (the core never panics on input).
pub(crate) fn parse(d: &str) -> Option<Path> {
    let mut s = Scanner {
        bytes: d.as_bytes(),
        at: 0,
    };
    let mut pb = PathBuilder::new();
    let mut cur: Pt = (0.0, 0.0);
    let mut start = cur;
    // Previous control point, for the smooth commands S and T.
    let (mut prev_cubic, mut prev_quad): (Option<Pt>, Option<Pt>) = (None, None);
    let mut cmd: Option<u8> = None;
    loop {
        s.skip_separators();
        let Some(c) = s.peek() else { break };
        if c.is_ascii_alphabetic() {
            cmd = Some(c);
            s.at += 1;
        } else if cmd.is_none() || !s.at_number() {
            return None;
        }
        let c = cmd?;
        let rel = c.is_ascii_lowercase();
        let (ox, oy) = if rel { cur } else { (0.0, 0.0) };
        let (mut cubic, mut quad) = (None, None);
        match c.to_ascii_uppercase() {
            b'Z' => {
                pb.close();
                cur = start;
                cmd = None;
            }
            b'M' => {
                let p = (s.num()? + ox, s.num()? + oy);
                pb.move_to(p.0, p.1);
                (cur, start) = (p, p);
                // Extra coordinate pairs after a moveto are linetos.
                cmd = Some(if rel { b'l' } else { b'L' });
            }
            b'L' => {
                cur = (s.num()? + ox, s.num()? + oy);
                pb.line_to(cur.0, cur.1);
            }
            b'H' => {
                cur.0 = s.num()? + ox;
                pb.line_to(cur.0, cur.1);
            }
            b'V' => {
                cur.1 = s.num()? + oy;
                pb.line_to(cur.0, cur.1);
            }
            b'C' | b'S' => {
                let c1 = if c.eq_ignore_ascii_case(&b'C') {
                    (s.num()? + ox, s.num()? + oy)
                } else {
                    reflect(prev_cubic, cur)
                };
                let c2 = (s.num()? + ox, s.num()? + oy);
                let end = (s.num()? + ox, s.num()? + oy);
                pb.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
                (cubic, cur) = (Some(c2), end);
            }
            b'Q' | b'T' => {
                let ctrl = if c.eq_ignore_ascii_case(&b'Q') {
                    (s.num()? + ox, s.num()? + oy)
                } else {
                    reflect(prev_quad, cur)
                };
                let end = (s.num()? + ox, s.num()? + oy);
                pb.quad_to(ctrl.0, ctrl.1, end.0, end.1);
                (quad, cur) = (Some(ctrl), end);
            }
            b'A' => {
                let radii = (s.num()?, s.num()?);
                let phi = s.num()?;
                let flags = (s.flag()?, s.flag()?);
                let end = (s.num()? + ox, s.num()? + oy);
                arc(&mut pb, cur, radii, phi, flags, end);
                cur = end;
            }
            _ => return None,
        }
        (prev_cubic, prev_quad) = (cubic, quad);
    }
    pb.finish()
}

/// The control point mirrored through the current point (or the point itself).
fn reflect(prev: Option<Pt>, cur: Pt) -> Pt {
    prev.map_or(cur, |p| (2.0 * cur.0 - p.0, 2.0 * cur.1 - p.1))
}

struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_separators(&mut self) {
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_whitespace() || b == b',')
        {
            self.at += 1;
        }
    }

    fn at_number(&self) -> bool {
        self.peek()
            .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+'))
    }

    fn num(&mut self) -> Option<f32> {
        self.skip_separators();
        let begin = self.at;
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.at += 1;
        }
        let mut dot = false;
        while let Some(b) = self.peek() {
            match b {
                b'0'..=b'9' => {}
                b'.' if !dot => dot = true,
                _ => break,
            }
            self.at += 1;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'-' | b'+')) {
                self.at += 1;
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.at += 1;
            }
        }
        std::str::from_utf8(&self.bytes[begin..self.at])
            .ok()?
            .parse()
            .ok()
    }

    /// Arc flags may be written without separators, so read one character.
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        let flag = match self.peek()? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.at += 1;
        Some(flag)
    }
}

/// Appends an elliptical arc as cubic Beziers of at most a quarter turn.
fn arc(
    pb: &mut PathBuilder,
    from: Pt,
    radii: Pt,
    phi_degrees: f32,
    (large, sweep): (bool, bool),
    to: Pt,
) {
    let (mut rx, mut ry) = (radii.0.abs(), radii.1.abs());
    if from == to {
        return;
    }
    if rx == 0.0 || ry == 0.0 {
        pb.line_to(to.0, to.1);
        return;
    }
    let (sin, cos) = phi_degrees.to_radians().sin_cos();
    let (dx, dy) = ((from.0 - to.0) / 2.0, (from.1 - to.1) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    // Scale radii up when they cannot span the endpoints.
    let lambda = (x1 / rx).powi(2) + (y1 / ry).powi(2);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        (rx, ry) = (rx * k, ry * k);
    }
    let numerator = (rx * ry).powi(2) - (rx * y1).powi(2) - (ry * x1).powi(2);
    let denominator = (rx * y1).powi(2) + (ry * x1).powi(2);
    let mut k = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        k = -k;
    }
    let (cxp, cyp) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let centre = (
        cos * cxp - sin * cyp + f32::midpoint(from.0, to.0),
        sin * cxp + cos * cyp + f32::midpoint(from.1, to.1),
    );
    let theta = ((y1 - cyp) / ry).atan2((x1 - cxp) / rx);
    let mut delta = ((-y1 - cyp) / ry).atan2((-x1 - cxp) / rx) - theta;
    if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    } else if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    }
    let segments = (delta.abs() / std::f32::consts::FRAC_PI_2 - 1e-3)
        .ceil()
        .max(1.0);
    let step = delta / segments;
    let t = 4.0 / 3.0 * (step / 4.0).tan();
    let on_ellipse = |a: f32| (rx * a.cos(), ry * a.sin());
    let tangent = |a: f32| (-rx * a.sin(), ry * a.cos());
    let map = |(x, y): Pt| (cos * x - sin * y + centre.0, sin * x + cos * y + centre.1);
    let mut a = theta;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "1..=4"
    )]
    for _ in 0..segments as u32 {
        let b = a + step;
        let (p0, p3) = (on_ellipse(a), on_ellipse(b));
        let (d0, d3) = (tangent(a), tangent(b));
        let c1 = map((p0.0 + t * d0.0, p0.1 + t * d0.1));
        let c2 = map((p3.0 - t * d3.0, p3.1 - t * d3.1));
        let end = map(p3);
        pb.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
        a = b;
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    // Internal unit tests: the parser is exercised end to end through the
    // toolbar goldens; these pin the parsing edge cases that goldens hide.
    #[test]
    fn parses_compact_numbers_flags_and_relative_arcs() {
        let path = parse("m7.643 15.69 7.774-7.773a2.357 2.357 0 1 0-3.334-3.334z").unwrap();
        let b = path.bounds();
        assert!(b.left() > 4.0 && b.right() < 18.0 && b.top() > 0.0);
        assert!(parse("M0 0a1 1 0 0 1 0 -1.41").is_some());
        assert!(parse("M0 0 L").is_none());
        assert!(parse("1 1").is_none());
    }

    #[test]
    fn a_half_circle_arc_reaches_its_apex() {
        let b = parse("M0 0a5 5 0 0 1 10 0").unwrap().bounds();
        assert!(
            (b.top() + 5.0).abs() < 0.05 && (b.right() - 10.0).abs() < 0.05,
            "{b:?}"
        );
    }
}
