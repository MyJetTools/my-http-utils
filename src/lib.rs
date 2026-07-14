#![allow(clippy::module_inception)]

// Our own derive/attribute macros (e.g. `#[http_input_field]`) emit fully-qualified
// `my_http_utils::…` paths so they resolve in downstream crates. This self-alias makes those same
// paths resolve when the macros are used *inside* this crate (as `PasswordHttpInputField` does).
extern crate self as my_http_utils;

mod decode_from_url_string;
pub mod query_string;
pub mod url_decoder;
pub mod url_encoded_data_reader;
pub mod url_encoder;
pub use decode_from_url_string::*;
mod url_builder;
pub use url_builder::*;
mod path_and_query_key;
pub use path_and_query_key::*;
mod path_and_query_key_case_insensitive;
pub use path_and_query_key_case_insensitive::*;
mod path_and_query_parser;
pub use path_and_query_parser::*;
pub mod body;
pub mod form_data_reader;
pub mod schema;

/// Re-exported so the derive-generated client body builder can reach `JsonObjectWriter` via a
/// fully-qualified `my_http_utils::my_json::…` path (consumers don't depend on `my-json` directly).
pub use my_json;

/// Server-independent HTTP request parsing (value type, conversions, error, body readers and the
/// `THttpRequest` abstraction) plus the runtime the derive-generated `parse` targets. Gated
/// behind the `server` feature so wasm clients that only build requests don't compile it.
#[cfg(feature = "server")]
pub mod http_input;

/// Derive & attribute macros for HTTP request models (`MyHttpInput`, `MyHttpObjectStructure`,
/// `MyHttpInputObjectStructure`, `MyHttpStringEnum`, `MyHttpIntegerEnum`, `http_input_field`).
///
/// This is the single supported entry point: consumers use them via `my_http_utils::macros::…`
/// rather than depending on the proc-macro crate directly. The generated code targets
/// `my_http_utils::schema::…`, so `my-http-utils` is the only dependency a model crate needs.
pub mod macros {
    pub use http_request_schema_macros::*;
}
