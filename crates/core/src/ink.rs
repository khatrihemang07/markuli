//! Ink and its Elements.

/// A position on the Overlay, in physical pixels, origin top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// One freehand stroke. Same concept as an Excalidraw freedraw element;
/// the pressure and JSON fields arrive with the tickets that need them.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    points: Vec<Point>,
    /// How many leading points the render step has already drawn.
    pub(crate) rendered: usize,
}

impl Element {
    pub(crate) fn start(at: Point) -> Self {
        Self {
            points: vec![at],
            rendered: 0,
        }
    }

    pub(crate) fn push(&mut self, at: Point) {
        if self.points.last() != Some(&at) {
            self.points.push(at);
        }
    }

    #[must_use]
    pub fn points(&self) -> &[Point] {
        &self.points
    }
}

/// The set of Elements on the Overlay. Memory only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ink {
    elements: Vec<Element>,
}

impl Ink {
    #[must_use]
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub(crate) fn elements_mut(&mut self) -> &mut [Element] {
        &mut self.elements
    }

    pub(crate) fn add(&mut self, element: Element) {
        self.elements.push(element);
    }

    pub(crate) fn pop(&mut self) -> Option<Element> {
        self.elements.pop()
    }

    /// Exchanges the whole Element list with `other` (Clear and its undo).
    pub(crate) fn swap_elements(&mut self, other: &mut Vec<Element>) {
        std::mem::swap(&mut self.elements, other);
    }

    /// Removes every Element and hands them back, for the operation log.
    pub(crate) fn take(&mut self) -> Vec<Element> {
        std::mem::take(&mut self.elements)
    }

    pub(crate) fn last_mut(&mut self) -> Option<&mut Element> {
        self.elements.last_mut()
    }
}

impl Ink {
    pub(crate) fn has_unrendered(&self) -> bool {
        self.elements.iter().any(|e| e.rendered < e.points.len())
    }
}
