use std::str::FromStr;

use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::de::DeserializeOwned;

use crate::form_data_reader::FormDataItem;
use crate::url_encoded_data_reader::UrlEncodedValue;

use super::data_src::{SRC_FORM_DATA, SRC_HEADER};
use super::json_encoded_data::JsonEncodedValueAsString;
use super::{convert_from_str, error::convert_reading_error, HttpParseError};

/// A single named value read out of an incoming request, before it is converted into a model
/// field's concrete type. This is the concrete port of the server's `EncodedParamValue` — the
/// value is deliberately **not** abstracted (a generic value would explode the where-bounds in
/// the derive and break validators), while the *sources* are abstracted behind
/// [`super::THttpRequest`].
///
/// * `UrlEncoded` — query string, path segments, and `x-www-form-urlencoded` bodies. The value
///   is percent-decoded on read.
/// * `Json` — a member of a JSON body object.
/// * `FormData` — a `multipart/form-data` part (value or file).
/// * `Plain` — a header value: taken verbatim (headers are not percent-encoded).
///
/// All the `TryInto<T>` conversions live in [`super::mappers`].
pub enum HttpInputValue<'s> {
    UrlEncoded {
        value: UrlEncodedValue<'s>,
        src: &'static str,
    },
    Json {
        name: &'s str,
        value: &'s JsonEncodedValueAsString<'s>,
        src: &'static str,
    },
    FormData {
        name: &'s str,
        value: &'s FormDataItem<'s>,
    },
    Plain {
        name: &'s str,
        value: &'s str,
        src: &'static str,
    },
}

impl<'s> HttpInputValue<'s> {
    pub fn from_url_encoded(value: UrlEncodedValue<'s>, src: &'static str) -> Self {
        Self::UrlEncoded { value, src }
    }

    /// Wraps a raw header value (no decoding).
    pub fn from_header(name: &'s str, value: &'s str) -> Self {
        Self::Plain {
            name,
            value,
            src: SRC_HEADER,
        }
    }

    pub fn get_name(&self) -> &str {
        match self {
            Self::UrlEncoded { value, .. } => value.get_name(),
            Self::Json { name, .. } => name,
            Self::FormData { name, .. } => name,
            Self::Plain { name, .. } => name,
        }
    }

    pub fn get_src(&self) -> &'static str {
        match self {
            Self::UrlEncoded { src, .. } => src,
            Self::Json { src, .. } => src,
            Self::FormData { .. } => SRC_FORM_DATA,
            Self::Plain { src, .. } => src,
        }
    }

    pub fn as_string(&self) -> Result<String, HttpParseError> {
        match self {
            Self::UrlEncoded { value, src } => {
                convert_reading_error(value.get_name(), value.as_string(), src)
            }
            Self::Json { value, .. } => value.as_string(),
            Self::FormData { value, .. } => Ok(value.unwrap_as_string()?.to_string()),
            Self::Plain { value, .. } => Ok(value.to_string()),
        }
    }

    pub fn parse<T: FromStr>(&self) -> Result<T, HttpParseError> {
        match self {
            Self::UrlEncoded { value, src } => {
                convert_reading_error(value.get_name(), value.parse(), src)
            }
            Self::Json { value, .. } => value.parse(),
            Self::FormData { name, value } => {
                convert_from_str::to_simple_value(name, value.unwrap_as_string()?, SRC_FORM_DATA)
            }
            Self::Plain { name, value, src } => convert_from_str::to_simple_value(name, value, src),
        }
    }

    pub fn as_bool(&self) -> Result<bool, HttpParseError> {
        match self {
            // Mirrors the server: bool reads the raw (still-escaped) query value — "true"/"1"
            // etc. contain no escapable characters, so decoding is unnecessary.
            Self::UrlEncoded { value, src } => convert_from_str::to_bool(value.get_name(), value.value, src),
            Self::Json { value, .. } => value.as_bool(),
            Self::FormData { name, value } => {
                convert_from_str::to_bool(name, value.unwrap_as_string()?, SRC_FORM_DATA)
            }
            Self::Plain { name, value, src } => convert_from_str::to_bool(name, value, src),
        }
    }

    pub fn as_date_time(&self) -> Result<DateTimeAsMicroseconds, HttpParseError> {
        match self {
            Self::UrlEncoded { value, src } => {
                let decoded = convert_reading_error(value.get_name(), value.as_str_or_string(), src)?;
                convert_from_str::to_date_time(value.get_name(), decoded.as_str(), src)
            }
            Self::Json { value, .. } => value.as_date_time(),
            Self::FormData { name, value } => {
                convert_from_str::to_date_time(name, value.unwrap_as_string()?, SRC_FORM_DATA)
            }
            Self::Plain { name, value, src } => convert_from_str::to_date_time(name, value, src),
        }
    }

    /// Deserialize the value's JSON representation into `T` (used for `Vec<T>` / `HashMap`).
    pub fn deserialize_json<T: DeserializeOwned>(&self) -> Result<T, HttpParseError> {
        match self {
            Self::UrlEncoded { value, src } => {
                let decoded = convert_reading_error(value.get_name(), value.as_string(), src)?;
                convert_from_str::to_json_from_slice(value.get_name(), decoded.as_bytes(), src)
            }
            Self::Json { value, .. } => value.deserialize(),
            Self::FormData { name, value } => convert_from_str::to_json_from_slice(
                name,
                value.unwrap_as_string()?.as_bytes(),
                SRC_FORM_DATA,
            ),
            Self::Plain { name, value, src } => {
                convert_from_str::to_json_from_slice(name, value.as_bytes(), src)
            }
        }
    }

    /// Raw bytes backing this value — used by the `RawData` / `RawDataTyped` conversions.
    pub(super) fn as_raw_bytes(&self) -> Result<Vec<u8>, HttpParseError> {
        match self {
            // The still-escaped query bytes, matching the server's RawData mapper.
            Self::UrlEncoded { value, .. } => Ok(value.value.as_bytes().to_vec()),
            Self::Json { value, .. } => Ok(value.as_bytes()),
            Self::FormData { value, .. } => Ok(value.unwrap_as_string()?.as_bytes().to_vec()),
            Self::Plain { value, .. } => Ok(value.as_bytes().to_vec()),
        }
    }
}
