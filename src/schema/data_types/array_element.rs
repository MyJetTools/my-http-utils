use super::{HttpEnumStructure, HttpObjectStructure, HttpSimpleType};

/// What one element of an `ArrayOf` / `DictionaryOf` / `DictionaryOfArray`
/// [`HttpDataType`](super::HttpDataType) holds.
///
/// The enum is recursive through [`ArrayElement::ArrayOf`], because array nesting in Rust is:
/// `Vec<Vec<T>>` is a `Vec` whose element is itself a `Vec`. Renderers mirror that recursion —
/// an `ArrayOf` element becomes another `{"type": "array", "items": …}` level, so
/// `Vec<Vec<Interval>>` renders as
/// `{"type":"array","items":{"type":"array","items":{"$ref": "…/Interval"}}}`.
#[derive(Clone, Debug)]
pub enum ArrayElement {
    SimpleType(HttpSimpleType),
    Object(HttpObjectStructure),
    Enum(HttpEnumStructure),
    /// An array of arrays: `Vec<Vec<T>>`, `Vec<Vec<Vec<T>>>`, … Boxed because the variant makes
    /// `ArrayElement` recursive.
    ArrayOf(Box<ArrayElement>),
}

impl ArrayElement {
    /// The element at the bottom of any `ArrayOf` nesting — i.e. the first element that is *not*
    /// another array. For a non-nested element this is `self`.
    ///
    /// This is what a renderer needs when it walks the schema to register definitions: only the
    /// innermost element can carry an object or an enum to name, however many array levels sit
    /// above it.
    pub fn innermost(&self) -> &Self {
        let mut current = self;
        while let Self::ArrayOf(inner) = current {
            current = inner.as_ref();
        }
        current
    }

    /// How many array levels this element adds on top of [`Self::innermost`]. `0` for a plain
    /// element, `1` for the element of a `Vec<Vec<T>>`, and so on.
    pub fn nested_array_depth(&self) -> usize {
        let mut depth = 0;
        let mut current = self;
        while let Self::ArrayOf(inner) = current {
            depth += 1;
            current = inner.as_ref();
        }
        depth
    }

    /// Wraps `self` into one more array level: the element of a `Vec` whose element type is
    /// already an array.
    pub fn into_array_of(self) -> Self {
        Self::ArrayOf(Box::new(self))
    }
}
