//! The request body as a whole. Backs `#[http_body_raw]` (non-Option) fields, whose value is
//! `body.raw_body()?.try_into()?`. Ported from `my-http-server-core` — `HttpRequestBodyContent`
//! (its `TryInto` set) and `BodyContentType` — dropping hyper and returning [`HttpParseError`].

use std::collections::HashMap;

use serde::de::DeserializeOwned;

use crate::url_encoded_data_reader::UrlEncodedDataReader;

use crate::http_input::{HttpParseError, RawData, RawDataTyped};

use super::convert_from_str;
use super::data_src::SRC_BODY;

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

/// The full request body, owned. Only `#[http_body_raw]` non-Option fields go through it (raw
/// bytes / string / a JSON deserialize of the whole body).
pub struct HttpRequestBodyContent {
    raw_body: Vec<u8>,
    #[allow(dead_code)]
    body_content_type: BodyContentType,
}

impl HttpRequestBodyContent {
    pub fn new(body: Vec<u8>, body_content_type: BodyContentType) -> Self {
        let body_content_type = if body_content_type.is_unknown_or_empty() {
            BodyContentType::detect_from_body(body.as_slice()).unwrap_or(body_content_type)
        } else {
            body_content_type
        };

        Self {
            raw_body: body,
            body_content_type,
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.raw_body
    }

    pub fn get_body(self) -> Vec<u8> {
        self.raw_body
    }

    pub fn as_str(&self) -> Result<&str, HttpParseError> {
        std::str::from_utf8(self.as_slice())
            .map_err(|err| HttpParseError::InvalidBodyFormat(format!("Body is not valid UTF-8: {}", err)))
    }
}

macro_rules! impl_body_try_into_simple {
    ($($t:ty),+ $(,)?) => {
        $(
            impl TryInto<$t> for HttpRequestBodyContent {
                type Error = HttpParseError;
                fn try_into(self) -> Result<$t, Self::Error> {
                    let str = self.as_str()?;
                    convert_from_str::to_simple_value("HttpBody", str, SRC_BODY)
                }
            }
        )+
    };
}

impl_body_try_into_simple!(u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, f32, f64);

impl<T: DeserializeOwned> TryInto<Vec<T>> for HttpRequestBodyContent {
    type Error = HttpParseError;
    fn try_into(self) -> Result<Vec<T>, Self::Error> {
        convert_from_str::to_json_from_slice("RawBody", self.as_slice(), SRC_BODY)
    }
}

impl<T: DeserializeOwned> TryInto<HashMap<String, T>> for HttpRequestBodyContent {
    type Error = HttpParseError;
    fn try_into(self) -> Result<HashMap<String, T>, Self::Error> {
        convert_from_str::to_json_from_slice("RawBody", self.as_slice(), SRC_BODY)
    }
}

impl<'s> TryInto<&'s str> for &'s HttpRequestBodyContent {
    type Error = HttpParseError;
    fn try_into(self) -> Result<&'s str, Self::Error> {
        self.as_str()
    }
}

impl TryInto<String> for HttpRequestBodyContent {
    type Error = HttpParseError;
    fn try_into(self) -> Result<String, Self::Error> {
        Ok(self.as_str()?.to_string())
    }
}

impl TryInto<RawData> for HttpRequestBodyContent {
    type Error = HttpParseError;
    fn try_into(self) -> Result<RawData, Self::Error> {
        Ok(RawData::new(self.get_body()))
    }
}

impl<T: DeserializeOwned> TryInto<RawDataTyped<T>> for HttpRequestBodyContent {
    type Error = HttpParseError;
    fn try_into(self) -> Result<RawDataTyped<T>, Self::Error> {
        Ok(RawDataTyped::new(self.get_body(), SRC_BODY))
    }
}
