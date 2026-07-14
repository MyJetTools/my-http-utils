use std::marker::PhantomData;
use std::str::FromStr;

use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;

use crate::http_input::core::convert_from_str;
use crate::http_input::core::data_src::SRC_BODY_JSON;
use crate::http_input::HttpParseError;

/// One named value read out of a JSON body object.
///
/// It keeps the member's **verbatim source text** (`serde_json::value::RawValue`) rather than a
/// pre-parsed `serde_json::Value`. That reproduces the original `my-json`-based reader 1:1: a
/// number's exact scale/precision survives (`100.00` stays `100.00`, a 128-bit integer isn't
/// rounded through `f64`), and `RawData`/`RawDataTyped` over a JSON member get the exact original
/// bytes (no key reordering, no whitespace/escape canonicalisation). Conversion happens only at
/// the leaf. Still wasm-safe (`serde_json` `raw_value` feature, enabled by `server`).
pub struct JsonEncodedValueAsString<'s> {
    name: String,
    raw: Box<RawValue>,
    _lifetime: PhantomData<&'s [u8]>,
}

impl<'s> JsonEncodedValueAsString<'s> {
    pub fn new(name: String, raw: Box<RawValue>) -> Self {
        Self {
            name,
            raw,
            _lifetime: PhantomData,
        }
    }

    pub fn get_name(&self) -> &str {
        self.name.as_str()
    }

    /// The verbatim source text of this member (`"100.00"`, `"\"hi\""`, `"{\"b\":1,\"a\":2}"`, …).
    fn raw_text(&self) -> &str {
        self.raw.get()
    }

    /// First byte of the value — classifies it without a full parse.
    fn first_byte(&self) -> Option<u8> {
        self.raw_text().as_bytes().first().copied()
    }

    /// The unescaped contents of a JSON string member.
    fn as_unescaped_string(&self) -> Result<String, HttpParseError> {
        serde_json::from_str::<String>(self.raw_text()).map_err(|err| HttpParseError::CanNotParseValue {
            name: self.name.clone(),
            src: SRC_BODY_JSON,
            value: format!("{}", err),
        })
    }

    /// Textual form of a scalar (string → unescaped; number/bool → verbatim literal). `None` for
    /// arrays/objects/null. This is what `parse` / `as_bool` / `as_date_time` read.
    fn scalar_text(&self) -> Option<Result<String, HttpParseError>> {
        match self.first_byte() {
            Some(b'"') => Some(self.as_unescaped_string()),
            Some(b't') | Some(b'f') | Some(b'-') | Some(b'0'..=b'9') => {
                Some(Ok(self.raw_text().to_string()))
            }
            _ => None,
        }
    }

    pub fn as_string(&self) -> Result<String, HttpParseError> {
        match self.first_byte() {
            Some(b'"') => self.as_unescaped_string(),
            // null never reaches here (null members are dropped when building JsonEncodedData);
            // numbers/bools/arrays/objects return their verbatim source text.
            Some(b'n') => Err(HttpParseError::required(&self.name, SRC_BODY_JSON)),
            _ => Ok(self.raw_text().to_string()),
        }
    }

    pub fn as_bool(&self) -> Result<bool, HttpParseError> {
        match self.scalar_text() {
            Some(text) => convert_from_str::to_bool(&self.name, &text?, SRC_BODY_JSON),
            None => Err(HttpParseError::required(&self.name, SRC_BODY_JSON)),
        }
    }

    pub fn parse<T: FromStr>(&self) -> Result<T, HttpParseError> {
        match self.scalar_text() {
            Some(text) => convert_from_str::to_simple_value(&self.name, &text?, SRC_BODY_JSON),
            None => Err(HttpParseError::required(&self.name, SRC_BODY_JSON)),
        }
    }

    pub fn as_date_time(&self) -> Result<DateTimeAsMicroseconds, HttpParseError> {
        match self.scalar_text() {
            Some(text) => convert_from_str::to_date_time(&self.name, &text?, SRC_BODY_JSON),
            None => Err(HttpParseError::required(&self.name, SRC_BODY_JSON)),
        }
    }

    /// Deserialize the member (scalar or compound) into `T`, from its raw source text — so a
    /// `Vec<u128>` / `HashMap` / struct keeps full numeric precision.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, HttpParseError> {
        serde_json::from_str(self.raw_text()).map_err(|err| HttpParseError::CanNotParseValue {
            name: self.name.clone(),
            src: SRC_BODY_JSON,
            value: format!("{}", err),
        })
    }

    /// The member's verbatim source bytes — for `RawData` / `RawDataTyped` fields, handed over
    /// untouched (no re-serialisation). For a JSON string member this is the quoted, escaped
    /// source form, matching the original reader.
    pub fn as_bytes(&self) -> Vec<u8> {
        self.raw_text().as_bytes().to_vec()
    }
}
