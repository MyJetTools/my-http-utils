mod client_writer;
pub mod docs;
mod generate;
pub mod http_input_props;
mod input_field;
pub use generate::generate;
pub use input_field::*;
mod http_field_attr;
pub use http_field_attr::*;

// Server-side `parse` codegen — only compiled when the `server` feature is on, so the default
// (client / wasm) build stays lean.
#[cfg(feature = "server")]
mod http_input_default_value;
#[cfg(feature = "server")]
pub use http_input_default_value::*;
#[cfg(feature = "server")]
mod parse_field_ext;
#[cfg(feature = "server")]
pub mod parse;
