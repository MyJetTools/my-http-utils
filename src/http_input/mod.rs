//! Server-independent HTTP request parsing.
//!
//! Layout convention: the **types** live at the root of this module — the error
//! ([`HttpParseError`]), the raw/file field types ([`RawData`], [`RawDataTyped`],
//! [`FileContent`]), the custom field types ([`PasswordHttpInputField`]), and — behind the
//! `server` feature — the concrete value type (`HttpInputValue`). All the **logic** — the
//! JSON/url-encoded/form-data body readers, the value→field conversions, and the one abstraction
//! the server implements (`core::THttpRequest`) — lives under [`self::core`].
//!
//! This is the runtime half of the derive-generated sync `parse`. It was ported out of
//! `my-http-server-core` so both the client (schema + request builder) and the server read the
//! same field markup, and so the whole parse layer is wasm-safe (no hyper/tokio; `serde_json` for
//! JSON).
//!
//! **Feature gating.** The module itself is *always* compiled: a model shared between a client and
//! a server names these field types (`RawDataTyped<T>` in a `#[http_body_raw]` field,
//! `PasswordHttpInputField`, …), and such a model must compile for a wasm client that does not
//! enable `server`. Only the **parse engine** is gated behind `server` — the value type
//! (`HttpInputValue`) and everything under [`self::core`] except the source tags
//! ([`self::core::data_src`]) and the one primitive converter the field types themselves need.
//!
//! (The `self::` prefixes are load-bearing: a bare `[`core`]` link resolves to Rust's builtin
//! `core` crate, not to this module's own `core`.)

pub mod core;

mod error;
mod file_content;
mod password;
mod raw_data;
mod raw_data_typed;
// The parse engine's value type: only a server reads values out of an incoming request.
#[cfg(feature = "server")]
mod value;

pub use error::HttpParseError;
pub use file_content::FileContent;
pub use password::PasswordHttpInputField;
pub use raw_data::RawData;
pub use raw_data_typed::RawDataTyped;
#[cfg(feature = "server")]
pub use value::HttpInputValue;
