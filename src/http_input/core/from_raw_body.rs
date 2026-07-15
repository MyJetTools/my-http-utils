use serde::de::DeserializeOwned;

use crate::http_input::core::data_src::SRC_BODY;
use crate::http_input::{HttpParseError, RawData, RawDataTyped};

/// Builds a non-`Option` `#[http_body_raw]` field out of the whole request body (the `Vec<u8>`
/// returned by [`super::read_raw_body`]). The derive's server-side `parse` calls it once for the
/// raw-body field.
///
/// It is a **crate-local** conversion trait, used deliberately instead of `TryFrom<Vec<u8>>`: that
/// leaves std's `From`/`TryFrom` free for [`RawDataTyped<T>`] to carry the *client* direction
/// `From<T>` (serialise a `T` into the outgoing body) without a coherence clash against a
/// `From<Vec<u8>>` for the same type. Every raw-body target implements it:
///
/// * `Vec<u8>` — the bytes as-is;
/// * `String` — a utf-8 check (same error the old `TryFrom<Vec<u8>>` path produced);
/// * [`RawData`] — verbatim bytes, no content-type parsing;
/// * [`RawDataTyped<T>`] — verbatim bytes; the JSON error is deferred to
///   [`RawDataTyped::deserialize_json`], exactly as before.
pub trait FromRawBody: Sized {
    fn from_raw_body(body: Vec<u8>) -> Result<Self, HttpParseError>;
}

impl FromRawBody for Vec<u8> {
    fn from_raw_body(body: Vec<u8>) -> Result<Self, HttpParseError> {
        Ok(body)
    }
}

impl FromRawBody for String {
    fn from_raw_body(body: Vec<u8>) -> Result<Self, HttpParseError> {
        String::from_utf8(body).map_err(Into::into)
    }
}

impl FromRawBody for RawData {
    fn from_raw_body(body: Vec<u8>) -> Result<Self, HttpParseError> {
        Ok(RawData::new(body))
    }
}

impl<T: DeserializeOwned> FromRawBody for RawDataTyped<T> {
    fn from_raw_body(body: Vec<u8>) -> Result<Self, HttpParseError> {
        Ok(RawDataTyped::new(body, SRC_BODY))
    }
}
