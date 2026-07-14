//! The parse engine behind the types at [`super`]: the body/query readers, the primitive
//! `&str -> T` converters, the source-name constants, the JSON-member machinery, the value→field
//! `TryInto` conversions ([`mappers`]), and [`THttpRequest`] — the transport-free request
//! abstraction the derive-generated `parse` reads through (the server implements it over its
//! concrete request; tests implement it over in-memory data).

pub mod data_src;
pub mod json_encoded_data;

pub(crate) mod convert_from_str;

mod body_reader;
mod content_type;
mod mappers;
mod query_reader;
mod request;

pub use content_type::{extract_web_form_boundary, BodyContentType};
pub use body_reader::BodyReader;
pub use json_encoded_data::{JsonEncodedData, JsonEncodedValueAsString};
pub use query_reader::QueryStringReader;
pub use request::{
    read_header_optional, read_header_required, read_path_value, read_raw_body, THttpRequest,
};
