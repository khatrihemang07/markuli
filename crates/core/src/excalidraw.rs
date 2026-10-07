//! Excalidraw clipboard JSON.

use crate::Element;

pub(crate) fn clipboard<'a>(_elements: impl Iterator<Item = &'a Element>) -> Option<String> {
    None
}
