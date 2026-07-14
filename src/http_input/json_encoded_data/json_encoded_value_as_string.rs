use std::marker::PhantomData;
use std::str::FromStr;

use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::http_input::data_src::SRC_BODY_JSON;
use crate::http_input::{convert_from_str, HttpParseError};

/// One named value read out of a JSON body object.
///
/// The server port of the original used `my-json` for zero-copy reads; here it holds an owned
/// `serde_json::Value` (wasm-safe, no extra dependency — `serde_json` is already used by the
/// client builder). The `'s` lifetime is retained for API parity with the original — a
/// [`super::JsonEncodedData`] hands out `&'s JsonEncodedValueAsString<'s>` — and to keep the
/// value covariant over the body's lifetime.
pub struct JsonEncodedValueAsString<'s> {
    name: String,
    value: Value,
    _lifetime: PhantomData<&'s [u8]>,
}

impl<'s> JsonEncodedValueAsString<'s> {
    pub fn new(name: String, value: Value) -> Self {
        Self {
            name,
            value,
            _lifetime: PhantomData,
        }
    }

    pub fn get_name(&self) -> &str {
        self.name.as_str()
    }

    /// Textual form of a scalar (string / bool / number); `None` for arrays/objects/null.
    /// This is what the `parse` / `as_bool` / `as_date_time` paths read.
    fn scalar_text(&self) -> Option<String> {
        match &self.value {
            Value::String(s) => Some(s.clone()),
            Value::Bool(b) => Some(b.to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Result<String, HttpParseError> {
        match &self.value {
            Value::String(s) => Ok(s.clone()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Number(n) => Ok(n.to_string()),
            // Null never reaches here (null fields are dropped when building JsonEncodedData);
            // arrays/objects fall back to their compact JSON text.
            Value::Null => Err(HttpParseError::required(&self.name, SRC_BODY_JSON)),
            other => Ok(other.to_string()),
        }
    }

    pub fn as_bool(&self) -> Result<bool, HttpParseError> {
        if let Value::Bool(b) = &self.value {
            return Ok(*b);
        }

        match self.scalar_text() {
            Some(text) => convert_from_str::to_bool(&self.name, &text, SRC_BODY_JSON),
            None => Err(HttpParseError::required(&self.name, SRC_BODY_JSON)),
        }
    }

    pub fn parse<T: FromStr>(&self) -> Result<T, HttpParseError> {
        match self.scalar_text() {
            Some(text) => convert_from_str::to_simple_value(&self.name, &text, SRC_BODY_JSON),
            None => Err(HttpParseError::required(&self.name, SRC_BODY_JSON)),
        }
    }

    pub fn as_date_time(&self) -> Result<DateTimeAsMicroseconds, HttpParseError> {
        match self.scalar_text() {
            Some(text) => convert_from_str::to_date_time(&self.name, &text, SRC_BODY_JSON),
            None => Err(HttpParseError::required(&self.name, SRC_BODY_JSON)),
        }
    }

    /// Deserialize the (possibly compound) value into `T` — used for `Vec<T>` / `HashMap` body
    /// fields.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, HttpParseError> {
        serde_json::from_value(self.value.clone()).map_err(|err| HttpParseError::CanNotParseValue {
            name: self.name.clone(),
            src: SRC_BODY_JSON,
            value: format!("{}", err),
        })
    }

    /// The value re-serialized to compact JSON bytes — for `RawData` / `RawDataTyped` fields
    /// that address a single JSON member.
    pub fn as_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.value).unwrap_or_default()
    }
}
