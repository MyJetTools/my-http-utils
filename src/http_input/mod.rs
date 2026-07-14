//! Server-independent HTTP request parsing.
//!
//! Layout convention: the **types** live at the root of this module — the concrete value type
//! ([`HttpInputValue`]), the error ([`HttpParseError`]), the raw/file field types
//! ([`RawData`], [`RawDataTyped`], [`FileContent`]), and the custom field types
//! ([`PasswordHttpInputField`]). All the **logic** — the JSON/url-encoded/form-data body readers,
//! the value→field conversions, and the one abstraction the server implements
//! ([`core::THttpRequest`]) — lives under [`core`].
//!
//! This is the runtime half of the derive-generated sync `parse`. It was ported out of
//! `my-http-server-core` so both the client (schema + request builder) and the server read the
//! same field markup, and so the whole parse layer is wasm-safe (no hyper/tokio; `serde_json` for
//! JSON).
//!
//! The module is gated behind the `server` cargo feature: wasm clients that only build requests
//! never compile it.

pub mod core;

mod error;
mod file_content;
mod password;
mod raw_data;
mod raw_data_typed;
mod value;

pub use error::HttpParseError;
pub use file_content::FileContent;
pub use password::PasswordHttpInputField;
pub use raw_data::RawData;
pub use raw_data_typed::RawDataTyped;
pub use value::HttpInputValue;
