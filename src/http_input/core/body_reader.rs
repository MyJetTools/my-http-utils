use crate::form_data_reader::{FormDataHeader, FormDataReader};
use crate::url_encoded_data_reader::{UrlEncodedDataReader, UrlEncodedValue};

use crate::http_input::error::convert_reading_error;
use crate::http_input::{FormDataProperty, HttpInputValue, HttpParseError};

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

    /// A `#[http_form_data]` field: the first value named `name`, built by `T`'s
    /// [`FormDataProperty`]. `None` when there is none.
    ///
    /// A multipart part is handed over as it was sent — its header and its content. A form-data
    /// model takes an `x-www-form-urlencoded` or JSON body too; a value out of one of those has no
    /// part header, so `T` gets the name only and the value's text (percent-decoded / unescaped).
    pub fn get_form_data<T: FormDataProperty>(
        &'s self,
        name: &'static str,
    ) -> Result<Option<T>, HttpParseError> {
        match &self.inner {
            ParsedBody::FormData(reader) => match reader.get_part(name) {
                Some(part) => T::from_form_data_content(part.header, part.content).map(Some),
                None => Ok(None),
            },
            ParsedBody::UrlEncoded(reader) => match reader.get_optional(name) {
                Some(value) => form_data_from_url_encoded(name, &value).map(Some),
                None => Ok(None),
            },
            ParsedBody::Json(reader) => match reader.get_optional(name) {
                Some(value) => form_data_from_text(name, &value.as_string()?).map(Some),
                None => Ok(None),
            },
            ParsedBody::Unknown | ParsedBody::Empty => Ok(None),
        }
    }

    /// [`Self::get_form_data`] for a field that has to be there — failing as [`Self::get_required`]
    /// fails when it is not.
    pub fn get_form_data_required<T: FormDataProperty>(
        &'s self,
        name: &'static str,
    ) -> Result<T, HttpParseError> {
        if let ParsedBody::Unknown = &self.inner {
            return Err(HttpParseError::InvalidBodyFormat(
                "Body has an unknown format; can not read named fields from it".to_string(),
            ));
        }

        self.get_form_data(name)?
            .ok_or_else(|| HttpParseError::required(name, SRC_BODY))
    }

    /// A `Vec<T>` `#[http_form_data]` field: every value named `name`, in order, each built by `T`'s
    /// [`FormDataProperty`]. None is an empty `Vec` — the client sends nothing for an empty one.
    ///
    /// Multipart: one part per item, all under the same name (several files, a multi-select).
    /// Url-encoded: the key repeated, bare or as `name[]`. JSON: the elements of an array member —
    /// or the member itself when it is not an array.
    pub fn get_form_data_vec<T: FormDataProperty>(
        &'s self,
        name: &'static str,
    ) -> Result<Vec<T>, HttpParseError> {
        match &self.inner {
            ParsedBody::FormData(reader) => reader
                .get_parts(name)
                .map(|part| T::from_form_data_content(part.header, part.content))
                .collect(),
            ParsedBody::UrlEncoded(reader) => reader
                .get_vec(name)
                .iter()
                .map(|value| form_data_from_url_encoded(name, value))
                .collect(),
            ParsedBody::Json(reader) => match reader.get_optional(name) {
                Some(value) => value
                    .as_text_items()?
                    .iter()
                    .map(|text| form_data_from_text(name, text))
                    .collect(),
                None => Ok(Vec::new()),
            },
            ParsedBody::Unknown | ParsedBody::Empty => Ok(Vec::new()),
        }
    }
}

/// A form-data field's value out of a body with no part headers: the name, and the text.
fn form_data_from_text<T: FormDataProperty>(
    name: &'static str,
    text: &str,
) -> Result<T, HttpParseError> {
    T::from_form_data_content(FormDataHeader::new(name), text.as_bytes())
}

fn form_data_from_url_encoded<T: FormDataProperty>(
    name: &'static str,
    value: &UrlEncodedValue,
) -> Result<T, HttpParseError> {
    let text = convert_reading_error(name, value.as_string(), SRC_BODY_URL_ENCODED)?;
    form_data_from_text(name, &text)
}
