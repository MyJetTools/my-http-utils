//! Names of the sources a value was read from. They travel inside
//! [`crate::http_input::HttpParseError`] and the `server`-gated `HttpInputValue` so an error can
//! say *where* a value came from, and so the server
//! can rebuild the exact same `HttpFailResult` text from a `HttpParseError`.
//!
//! Ported verbatim from `my-http-server-core::data_src` (plus `SRC_PATH`).

pub const SRC_BODY: &str = "Body";
pub const SRC_BODY_JSON: &str = "BodyJson";
pub const SRC_BODY_URL_ENCODED: &str = "BodyUrlEncoded";
pub const SRC_QUERY_STRING: &str = "QueryString";
pub const SRC_HEADER: &str = "Header";
pub const SRC_FORM_DATA: &str = "FormData";
pub const SRC_PATH: &str = "Path";
