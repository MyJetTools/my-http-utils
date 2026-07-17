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

// The runtime half of the derive-generated `JsonValueReader`. At the crate root because the
// generated code names it as `my_http_utils::read_json_object_field`, exactly as it names
// `my_http_utils::my_json::…` for the writer.
mod json_object_reader;
pub use json_object_reader::read_json_object_field;

/// Re-exported so the derive-generated client body builder can reach `JsonObjectWriter` via a
/// fully-qualified `my_http_utils::my_json::…` path (consumers don't depend on `my-json` directly).
pub use my_json;

/// Re-exported for the same reason as [`my_json`]: the `MyHttpStringEnum` / `MyHttpIntegerEnum`
/// derives emit `Serialize`/`Deserialize` impls, and they reach the traits through
/// `my_http_utils::serde::…` so a model crate needs no direct `serde` dependency of its own.
pub use serde;

// Server-independent HTTP request parsing: the field types a model names, plus (behind the
// `server` feature) the parse engine the derive-generated `parse` targets. Always compiled — a
// model shared between a client and a server names those field types, so it must also compile for
// a wasm client that does not enable `server`; only the engine is gated. See the module's own docs
// in `src/http_input/mod.rs` — deliberately NOT an outer `///` block here, because rustdoc merges
// an outer block on the declaration with the module's inner `//!` one and then resolves the merged
// text in the crate-root scope, where the module's own items are not nameable.
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
