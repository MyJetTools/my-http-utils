//! Primitive `&str -> T` conversions, all returning [`HttpParseError`]. Ported from
//! `my-http-server-core::convert_from_str`, with `serde_json` for the JSON paths (wasm-safe).
//!
//! All of these are engine-only and gated on `server`, except [`to_json_from_slice`]: it is what
//! [`crate::http_input::RawDataTyped::deserialize_json`] calls, and that type is nameable (and
//! usable) by a client that does not enable `server`.

#[cfg(feature = "server")]
use std::str::FromStr;

#[cfg(feature = "server")]
use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::de::DeserializeOwned;

use crate::http_input::HttpParseError;

#[cfg(feature = "server")]
pub fn to_simple_value<T: FromStr>(
    name: &str,
    value: &str,
    src: &'static str,
) -> Result<T, HttpParseError> {
    match value.parse() {
        Ok(result) => Ok(result),
        Err(_) => Err(HttpParseError::cannot_parse(name, src, value)),
    }
}

#[cfg(feature = "server")]
pub fn to_bool(name: &str, value: &str, src: &'static str) -> Result<bool, HttpParseError> {
    if value == "1" || value.eq_ignore_ascii_case("true") {
        return Ok(true);
    }

    if value == "0" || value.eq_ignore_ascii_case("false") {
        return Ok(false);
    }

    Err(HttpParseError::cannot_parse(name, src, value))
}

#[cfg(feature = "server")]
pub fn to_date_time(
    name: &str,
    value: &str,
    src: &'static str,
) -> Result<DateTimeAsMicroseconds, HttpParseError> {
    match DateTimeAsMicroseconds::from_str(value) {
        Some(result) => Ok(result),
        None => Err(HttpParseError::cannot_parse(name, src, value)),
    }
}

pub fn to_json_from_slice<T: DeserializeOwned>(
    name: &str,
    value: &[u8],
    src: &'static str,
) -> Result<T, HttpParseError> {
    match serde_json::from_slice(value) {
        Ok(result) => Ok(result),
        Err(_) => Err(HttpParseError::cannot_parse(
            name,
            src,
            String::from_utf8_lossy(value),
        )),
    }
}
