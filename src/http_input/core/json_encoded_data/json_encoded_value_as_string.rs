use std::str::FromStr;

use my_json::json_reader::JsonValueRef;
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::de::DeserializeOwned;

use crate::http_input::core::convert_from_str;
use crate::http_input::core::data_src::SRC_BODY_JSON;
use crate::http_input::HttpParseError;

/// One named value read out of a JSON body object.
///
/// Zero-copy: it wraps a `my_json::JsonValueRef` that borrows the member's bytes straight out of
/// the request body — no per-member `Box`/`String`, no pre-parsed map. The member's **verbatim
/// source text** survives: a number keeps its exact scale (`100.00`, not f64-rounded `100.0`) and
/// its full width (a 128-bit integer is not squeezed through `i64`), and an object/array is handed
/// over byte-for-byte. Scalars are converted from their raw text (via [`convert_from_str`]);
/// object/array/wide-number values are deserialized from the raw slice by `serde_json` at the leaf
/// (`my-json` itself has no serde dependency). Replaces the earlier `serde_json::value::RawValue`
/// reader, so the crate no longer needs `serde_json/raw_value`.
pub struct JsonEncodedValueAsString<'s> {
    name: &'s str,
    value: JsonValueRef<'s>,
}

impl<'s> JsonEncodedValueAsString<'s> {
    pub fn new(name: &'s str, value: JsonValueRef<'s>) -> Self {
        Self { name, value }
    }

    pub fn get_name(&self) -> &str {
        self.name
    }

    /// The member's verbatim source text — the raw JSON bytes exactly as they appeared in the body
    /// (`100.00`, `"hi"`, `{"b":1,"a":2}`, …).
    fn raw_text(&self) -> &str {
        std::str::from_utf8(self.value.as_slice()).unwrap_or("")
    }

    /// The unescaped contents of a JSON string member (quotes stripped, escapes resolved).
    fn as_unescaped_string(&self) -> Result<String, HttpParseError> {
        match self.value.as_unescaped_str() {
            Some(text) => Ok(text.to_string()),
            None => Err(HttpParseError::required(self.name, SRC_BODY_JSON)),
        }
    }

    /// Textual form of a scalar (string → unescaped; number/bool → verbatim literal). `None` for
    /// arrays/objects/null. This is what `parse` / `as_bool` / `as_date_time` read.
    fn scalar_text(&self) -> Option<Result<String, HttpParseError>> {
        if self.value.is_string() {
            Some(self.as_unescaped_string())
        } else if self.value.is_number() || self.value.is_double() || self.value.is_bool() {
            Some(Ok(self.raw_text().to_string()))
        } else {
            None
        }
    }

    pub fn as_string(&self) -> Result<String, HttpParseError> {
        if self.value.is_string() {
            self.as_unescaped_string()
        } else if self.value.is_null() {
            // null never reaches here (null members are dropped when building JsonEncodedData).
            Err(HttpParseError::required(self.name, SRC_BODY_JSON))
        } else {
            // numbers/bools/arrays/objects return their verbatim source text.
            Ok(self.raw_text().to_string())
        }
    }

    pub fn as_bool(&self) -> Result<bool, HttpParseError> {
        match self.scalar_text() {
            Some(text) => convert_from_str::to_bool(self.name, &text?, SRC_BODY_JSON),
            None => Err(HttpParseError::required(self.name, SRC_BODY_JSON)),
        }
    }

    pub fn parse<T: FromStr>(&self) -> Result<T, HttpParseError> {
        match self.scalar_text() {
            Some(text) => convert_from_str::to_simple_value(self.name, &text?, SRC_BODY_JSON),
            None => Err(HttpParseError::required(self.name, SRC_BODY_JSON)),
        }
    }

    pub fn as_date_time(&self) -> Result<DateTimeAsMicroseconds, HttpParseError> {
        match self.scalar_text() {
            Some(text) => convert_from_str::to_date_time(self.name, &text?, SRC_BODY_JSON),
            None => Err(HttpParseError::required(self.name, SRC_BODY_JSON)),
        }
    }

    /// Deserialize the member (object / array / wide number) into `T` from its verbatim source
    /// slice. `my-json` carries no serde dependency, so compound values are handed to `serde_json`
    /// here, at the leaf — reading the raw bytes keeps full numeric precision (e.g. `Vec<u128>`).
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, HttpParseError> {
        serde_json::from_slice(self.value.as_slice()).map_err(|err| {
            HttpParseError::CanNotParseValue {
                name: self.name.to_string(),
                src: SRC_BODY_JSON,
                value: format!("{}", err),
            }
        })
    }

    /// The member's verbatim source bytes — for `RawData` / `RawDataTyped` fields, handed over
    /// untouched (no key reordering, no re-escaping). For a JSON string member this is the quoted,
    /// escaped source form, matching the original reader.
    pub fn as_bytes(&self) -> Vec<u8> {
        self.value.as_slice().to_vec()
    }
}
