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
//! same field markup, and so the whole parse layer is wasm-safe (no hyper; `serde_json` for JSON,
//! and `tokio/sync` for the body-stream channel — no runtime, no transport).
//!
//! **Feature gating.** The module itself is *always* compiled: a model shared between a client and
//! a server names these field types (`RawDataTyped<T>` in a `#[http_body_raw]` field,
//! `HttpBodyAsStream` in a `#[http_body_as_stream]` one, `PasswordHttpInputField`, …), and such a
//! model must compile for a wasm client that does not enable `server`. Only the **parse engine** is
//! gated behind `server` — the value type (`HttpInputValue`) and everything under [`self::core`]
//! except the source tags ([`self::core::data_src`]) and the one primitive converter the field
//! types themselves need.
//!
//! [`HttpBodyAsStream`] and its channel ([`HttpBodyStreamSender`] / [`HttpBodyReader`]) are
//! **not** gated either — the same channel carries an *incoming* body on the server and an
//! *outgoing* one on the client, so a wasm client that never enables `server` still needs all of
//! it. Its only `server`-only part is the OpenAPI `DataTypeProvider` impl.
//!
//! (The `self::` prefixes are load-bearing: a bare `[`core`]` link resolves to Rust's builtin
//! `core` crate, not to this module's own `core`.)

pub mod core;

mod body_as_stream;
mod error;
mod file_content;
mod password;
mod raw_data;
mod raw_data_typed;
// The parse engine's value type: only a server reads values out of an incoming request.
#[cfg(feature = "server")]
mod value;

// All ungated: the channel carries a body in BOTH directions — a server reading an incoming
// `#[http_body_as_stream]` field, and a client streaming an outgoing body out of the same model —
// so a wasm client that does not enable `server` needs the whole thing.
pub use body_as_stream::{
    HttpBodyAsStream, HttpBodyReader, HttpBodyStreamSender, BODY_STREAM_DEFAULT_BUFFER,
};
pub use error::HttpParseError;
pub use file_content::FileContent;
pub use password::PasswordHttpInputField;
pub use raw_data::RawData;
pub use raw_data_typed::RawDataTyped;
#[cfg(feature = "server")]
pub use value::HttpInputValue;
