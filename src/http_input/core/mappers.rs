//! `TryInto<T>` for [`HttpInputValue`], porting `my-http-server-core::encoded_value::mappers`
//! (and `file_content`) to return [`HttpParseError`]. These are what the derive-generated
//! `parse` calls via `value.try_into()?`.

use std::collections::HashMap;

use rust_extensions::date_time::DateTimeAsMicroseconds;
use serde::de::DeserializeOwned;

use crate::form_data_reader::FormDataItem;

use super::types::{FileContent, RawData, RawDataTyped};
use super::{HttpInputValue, HttpParseError};

impl<'s> TryInto<String> for HttpInputValue<'s> {
    type Error = HttpParseError;
    fn try_into(self) -> Result<String, Self::Error> {
        self.as_string()
    }
}

impl<'s> TryInto<bool> for HttpInputValue<'s> {
    type Error = HttpParseError;
    fn try_into(self) -> Result<bool, Self::Error> {
        self.as_bool()
    }
}

macro_rules! impl_try_into_simple {
    ($($t:ty),+ $(,)?) => {
        $(
            impl<'s> TryInto<$t> for HttpInputValue<'s> {
                type Error = HttpParseError;
                fn try_into(self) -> Result<$t, Self::Error> {
                    self.parse()
                }
            }
        )+
    };
}

impl_try_into_simple!(u8, i8, u16, i16, u32, i32, u64, i64, usize, isize, f32, f64);

impl<'s> TryInto<DateTimeAsMicroseconds> for HttpInputValue<'s> {
    type Error = HttpParseError;
    fn try_into(self) -> Result<DateTimeAsMicroseconds, Self::Error> {
        self.as_date_time()
    }
}

impl<'s, T: DeserializeOwned> TryInto<Vec<T>> for HttpInputValue<'s> {
    type Error = HttpParseError;
    fn try_into(self) -> Result<Vec<T>, Self::Error> {
        self.deserialize_json()
    }
}

impl<'s, TValue: DeserializeOwned> TryInto<HashMap<String, TValue>> for HttpInputValue<'s> {
    type Error = HttpParseError;
    fn try_into(self) -> Result<HashMap<String, TValue>, Self::Error> {
        match &self {
            // A url-encoded scalar can not be read as a keyed map (matches the server).
            HttpInputValue::UrlEncoded { src, .. } | HttpInputValue::Plain { src, .. } => {
                Err(HttpParseError::NotSupportedContentType(format!(
                    "Can not read a map out of a raw value in {}",
                    src
                )))
            }
            _ => self.deserialize_json(),
        }
    }
}

impl<'s> TryInto<RawData> for HttpInputValue<'s> {
    type Error = HttpParseError;
    fn try_into(self) -> Result<RawData, Self::Error> {
        Ok(RawData::new(self.as_raw_bytes()?))
    }
}

impl<'s, T: DeserializeOwned> TryInto<RawDataTyped<T>> for HttpInputValue<'s> {
    type Error = HttpParseError;
    fn try_into(self) -> Result<RawDataTyped<T>, Self::Error> {
        let src = self.get_src();
        Ok(RawDataTyped::new(self.as_raw_bytes()?, src))
    }
}

impl<'s> TryInto<FileContent> for HttpInputValue<'s> {
    type Error = HttpParseError;
    fn try_into(self) -> Result<FileContent, Self::Error> {
        match self {
            HttpInputValue::FormData { value, .. } => match value {
                FormDataItem::File {
                    file_name,
                    content_type,
                    content,
                    ..
                } => Ok(FileContent {
                    content_type: content_type.to_string(),
                    file_name: file_name.to_string(),
                    content: content.to_vec(),
                }),
                FormDataItem::ValueAsString { name, .. } => {
                    Err(HttpParseError::NotSupportedContentType(format!(
                        "Field '{}' is a form-data value, not a file",
                        name
                    )))
                }
            },
            // A file can not be read out of a query-string / header / JSON value — the reference
            // server answered 403 here, so use Forbidden (not the 415 NotSupportedContentType).
            other => Err(HttpParseError::Forbidden(format!(
                "Can not read a file out of {}",
                other.get_src()
            ))),
        }
    }
}
