//! The one runtime helper the derive-generated `JsonValueReader` leans on.
//!
//! Kept here rather than inlined into the generated code so the absent-vs-null rule lives in one
//! readable place instead of being copy-pasted into every model.

use my_json::json_reader::{JsonParseError, JsonValueReader};

/// Reads one named member out of an object's verbatim source slice.
///
/// `raw` is the object's own bytes (`JsonValueRef::as_slice()`), borrowed for `'s`, so a member
/// that is itself an object recurses into the same generated impl.
///
/// Absent and `null` are treated alike, and both go through
/// [`JsonValueReader::from_absent_json_value`] — which is `Ok(None)` for `Option<T>` and an error
/// for everything else. That mirrors the writer: `JsonObjectWriter::write_if_some` omits the key
/// entirely for `None`, so the reader has to be able to read nothing back. Treating an explicit
/// `null` the same way matches what the previous serde-based reader did for `Option<T>`, so a body
/// that sends `"x": null` keeps meaning "absent".
pub fn read_json_object_field<'s, T: JsonValueReader<'s>>(
    raw: &'s [u8],
    field_name: &str,
) -> Result<T, JsonParseError> {
    match my_json::j_path::get_value(raw, field_name)? {
        Some(value) if !value.is_null() => T::from_json_value(&value),
        _ => T::from_absent_json_value(field_name),
    }
}
