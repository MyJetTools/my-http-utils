//! The parse engine behind the types at [`super`]: the body/query readers, the primitive
//! `&str -> T` converters, the source-name constants, the JSON-member machinery, the value→field
//! `TryInto` conversions (`mappers`), and `THttpRequest` — the transport-free request
//! abstraction the derive-generated `parse` reads through (the server implements it over its
//! concrete request; tests implement it over in-memory data).
//!
//! **Feature gating.** The module is always compiled, but almost all of it sits behind the
//! `server` feature. Only the two pieces the *field types* at [`super`] depend on are
//! unconditional:
//!
//! * [`data_src`] — plain `&'static str` tags carried by [`super::HttpParseError`] and by
//!   [`super::RawDataTyped`]; no logic, no dependencies.
//! * `convert_from_str::to_json_from_slice` — what [`super::RawDataTyped::deserialize_json`]
//!   calls. The remaining converters (`to_simple_value` / `to_bool` / `to_date_time`) are read
//!   only by the engine and are gated with it, so a client build carries none of them.

pub mod data_src;

pub(crate) mod convert_from_str;

#[cfg(feature = "server")]
pub mod json_encoded_data;

#[cfg(feature = "server")]
mod body_reader;
#[cfg(feature = "server")]
mod content_type;
#[cfg(feature = "server")]
mod from_raw_body;
#[cfg(feature = "server")]
mod mappers;
#[cfg(feature = "server")]
mod query_reader;
#[cfg(feature = "server")]
mod request;

#[cfg(feature = "server")]
pub use content_type::{extract_web_form_boundary, BodyContentType};
#[cfg(feature = "server")]
pub use body_reader::BodyReader;
#[cfg(feature = "server")]
pub use from_raw_body::FromRawBody;
#[cfg(feature = "server")]
pub use json_encoded_data::{JsonEncodedData, JsonEncodedValueAsString};
#[cfg(feature = "server")]
pub use query_reader::QueryStringReader;
#[cfg(feature = "server")]
pub use request::{
    read_header_optional, read_header_required, read_path_value, read_raw_body, THttpRequest,
};
