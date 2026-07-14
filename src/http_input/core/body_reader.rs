use crate::form_data_reader::FormDataReader;
use crate::url_encoded_data_reader::UrlEncodedDataReader;

use crate::http_input::{HttpInputValue, HttpParseError};

use super::content_type::BodyContentType;
use super::data_src::{SRC_BODY, SRC_BODY_JSON, SRC_BODY_URL_ENCODED};
use super::json_encoded_data::JsonEncodedData;

enum ParsedBody<'s> {
    UrlEncoded(UrlEncodedDataReader<'s>),
    Json(JsonEncodedData<'s>),
    FormData(FormDataReader<'s>),
    Unknown,
    Empty,
}

/// Reads NAMED model fields out of a request body — JSON, `x-www-form-urlencoded`, or
/// `multipart/form-data`, dispatched by content type. Built by the derive-generated `parse`
/// from `request.get_body()` + `request.get_content_type()`. (A non-Option `#[http_body_raw]`
/// field does not use this — it takes the whole body via [`super::read_raw_body`], which never
/// parses per content-type.)
pub struct BodyReader<'s> {
    inner: ParsedBody<'s>,
}

impl<'s> BodyReader<'s> {
    pub fn from_parts(raw: &'s [u8], content_type: Option<&str>) -> Result<Self, HttpParseError> {
        let mut content_type = match content_type {
            Some(header) => BodyContentType::from_content_type(header)?,
            None if raw.is_empty() => BodyContentType::Empty,
            None => BodyContentType::Unknown,
        };

        if content_type.is_unknown_or_empty() {
            if let Some(detected) = BodyContentType::detect_from_body(raw) {
                content_type = detected;
            }
        }

        let inner = match &content_type {
            BodyContentType::Json => ParsedBody::Json(JsonEncodedData::from_slice(raw)?),
            BodyContentType::UrlEncoded => {
                let body = std::str::from_utf8(raw).map_err(|err| {
                    HttpParseError::InvalidBodyFormat(format!(
                        "url-encoded body is not valid UTF-8: {}",
                        err
                    ))
                })?;
                ParsedBody::UrlEncoded(UrlEncodedDataReader::new(body)?)
            }
            // `boundary` is borrowed from the local `content_type` only for the duration of this
            // call — `FormDataReader` copies it and retains a borrow of `raw` alone.
            BodyContentType::FormData(boundary) => {
                ParsedBody::FormData(FormDataReader::new(raw, boundary))
            }
            BodyContentType::Unknown => ParsedBody::Unknown,
            BodyContentType::Empty => ParsedBody::Empty,
        };

        let _ = content_type; // consumed while building `inner`; not retained
        Ok(Self { inner })
    }

    pub fn get_optional(&'s self, name: &'static str) -> Option<HttpInputValue<'s>> {
        match &self.inner {
            ParsedBody::UrlEncoded(reader) => reader
                .get_optional(name)
                .map(|value| HttpInputValue::from_url_encoded(value, SRC_BODY_URL_ENCODED)),
            ParsedBody::Json(reader) => reader.get_optional(name).map(|value| HttpInputValue::Json {
                name,
                value,
                src: SRC_BODY_JSON,
            }),
            ParsedBody::FormData(reader) => reader
                .get_optional(name)
                .map(|value| HttpInputValue::FormData { name, value }),
            ParsedBody::Unknown | ParsedBody::Empty => None,
        }
    }

    pub fn get_required(&'s self, name: &'static str) -> Result<HttpInputValue<'s>, HttpParseError> {
        match &self.inner {
            ParsedBody::Unknown => Err(HttpParseError::InvalidBodyFormat(
                "Body has an unknown format; can not read named fields from it".to_string(),
            )),
            ParsedBody::Empty => Err(HttpParseError::required(name, SRC_BODY)),
            _ => self
                .get_optional(name)
                .ok_or_else(|| HttpParseError::required(name, SRC_BODY)),
        }
    }
}
