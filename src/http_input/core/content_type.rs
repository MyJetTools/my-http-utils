//! Interpreting a request body's `Content-Type`: [`BodyContentType`] (how to parse the body —
//! JSON / url-encoded / multipart / unknown / empty) and [`extract_web_form_boundary`] (the
//! multipart boundary). That's all `my-http-utils` needs from the content type — the raw body
//! itself is handed to a `#[http_body_raw]` field verbatim as `Vec<u8>` (see
//! [`super::read_raw_body`]), never dispatched on its content type.

use crate::http_input::HttpParseError;
use crate::url_encoded_data_reader::UrlEncodedDataReader;

/// How a body should be parsed, derived from its `Content-Type` (or sniffed from the bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyContentType {
    Json,
    UrlEncoded,
    FormData(String),
    Unknown,
    Empty,
}

impl BodyContentType {
    pub fn is_unknown_or_empty(&self) -> bool {
        matches!(self, Self::Unknown | Self::Empty)
    }

    pub fn from_content_type(content_type: &str) -> Result<Self, HttpParseError> {
        if content_type.is_empty() {
            return Ok(Self::Unknown);
        }

        let lower_case = content_type.to_lowercase();
        if lower_case.contains("json") {
            return Ok(Self::Json);
        }

        if lower_case.contains("x-www-form-urlencoded") {
            return Ok(Self::UrlEncoded);
        }

        if lower_case.contains("multipart/form-data") {
            return match extract_web_form_boundary(content_type) {
                Some(boundary) => Ok(Self::FormData(boundary.to_string())),
                None => Err(HttpParseError::InvalidBodyFormat(format!(
                    "Can not extract FormData boundary from content type '{}'",
                    content_type
                ))),
            };
        }

        Ok(Self::Unknown)
    }

    /// Best-effort content-type detection from the body bytes (JSON if it starts with `{`/`[`,
    /// url-encoded if it parses as such). Used when no `Content-Type` was provided.
    pub fn detect_from_body(raw_body: &[u8]) -> Option<Self> {
        for b in raw_body {
            if *b <= 32 {
                continue;
            }

            if *b == b'{' || *b == b'[' {
                return Some(Self::Json);
            } else {
                break;
            }
        }

        if let Ok(body_as_str) = std::str::from_utf8(raw_body) {
            if body_as_str.contains('=') && UrlEncodedDataReader::new(body_as_str).is_ok() {
                return Some(Self::UrlEncoded);
            }
        }

        None
    }
}

pub fn extract_web_form_boundary(content_type: &str) -> Option<&str> {
    for item in content_type.split(';') {
        let item = item.trim();
        if let Some(boundary) = item.strip_prefix("boundary=") {
            return Some(boundary);
        }
    }

    None
}
