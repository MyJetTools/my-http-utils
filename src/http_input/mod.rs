//! Server-independent HTTP request parsing.
//!
//! This is the runtime half of the derive-generated sync `parse`: the concrete value type
//! ([`HttpInputValue`]) with all its `TryInto<T>` conversions, the error ([`HttpParseError`]),
//! the JSON/url-encoded/form-data body readers, and the one abstraction the server implements —
//! [`THttpRequest`]. It was ported out of `my-http-server-core` so both the client (schema +
//! request builder) and the server read the same field markup, and so the whole parse layer is
//! wasm-safe (no hyper/tokio; `serde_json` for JSON).
//!
//! The module is gated behind the `server` cargo feature: wasm clients that only build requests
//! never compile it.

pub mod data_src;
pub mod json_encoded_data;
pub mod types;

mod body_content;
mod body_reader;
mod convert_from_str;
mod error;
mod mappers;
mod query_reader;
mod request;
mod value;

pub use body_content::{extract_web_form_boundary, BodyContentType, HttpRequestBodyContent};
pub use body_reader::BodyReader;
pub use error::HttpParseError;
pub use json_encoded_data::{JsonEncodedData, JsonEncodedValueAsString};
pub use query_reader::QueryStringReader;
pub use request::{read_header_optional, read_header_required, read_path_value, THttpRequest};
pub use types::{FileContent, RawData, RawDataTyped};
pub use value::HttpInputValue;
