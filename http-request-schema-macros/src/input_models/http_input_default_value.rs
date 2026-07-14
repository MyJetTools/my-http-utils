/// Wrapper around a field's `default = …` attribute value, mirroring
/// `my-http-server-macros::HttpInputDefaultValue`. Distinguishes `default` with a literal value
/// from bare `default` (no value → the type's `create_default()`).
pub struct HttpInputDefaultValue<'s> {
    value: types_reader::AnyValue<'s>,
}

impl<'s> HttpInputDefaultValue<'s> {
    pub fn new(value: types_reader::AnyValue<'s>) -> Self {
        Self { value }
    }

    pub fn has_empty_value(&self) -> bool {
        self.value.has_no_value()
    }

    pub fn get_value(&'s self) -> &'s types_reader::AnyValue<'s> {
        &self.value
    }
}
