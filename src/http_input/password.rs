//! A custom String-backed input field rendered as an OpenAPI `password` (masked in Swagger UI).
//!
//! Defined through this crate's own [`http_input_field`](crate::macros::http_input_field)
//! attribute, so both the schema (`DataTypeProvider` → `HttpSimpleType::Password`) and the parse
//! (`TryFrom<HttpInputValue>`) are owned here in `my-http-utils` — a model with a
//! `PasswordHttpInputField` field compiles the same on the server and on a wasm client (where the
//! server-gated `TryFrom` / `DataTypeProvider` are simply not emitted).
//!
//! The value is passed through verbatim (matching the previous server-side field); add validation
//! here if it is ever needed.

use std::ops::Deref;

use crate::macros::http_input_field;

// The attribute emits: `#[derive(Debug)] pub struct PasswordHttpInputField(String);`, the
// `DataTypeProvider` (Password) + `TryFrom<HttpInputValue>` impls (server-gated), `new(impl
// Into<String>)`, `as_str()`, and `Into<String>`. The rest of the public surface is added below.
#[http_input_field(open_api_type: "Password")]
pub struct PasswordHttpInputField(String);

impl PasswordHttpInputField {
    pub fn into_string(self) -> String {
        self.0
    }
}

impl Deref for PasswordHttpInputField {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<String> for PasswordHttpInputField {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
