use std::str::FromStr;

use rust_extensions::StrOrString;

use super::ReadingEncodedDataError;

#[derive(Clone)]
pub struct UrlEncodedValue<'s> {
    name: String,
    pub value: &'s str,
}

impl<'s> UrlEncodedValue<'s> {
    pub fn new(name: String, value: &'s str) -> Self {
        Self { name, value }
    }

    pub fn get_name(&self) -> &str {
        if self.name.ends_with("[]") {
            return &self.name[..self.name.len() - 2];
        }

        &self.name
    }

    pub fn as_string(&self) -> Result<String, ReadingEncodedDataError> {
        let result = crate::url_decoder::decode_from_url_query_string(self.value)?;
        Ok(result)
    }

    pub fn as_str_or_string(&'s self) -> Result<StrOrString<'s>, ReadingEncodedDataError> {
        let result = crate::url_decoder::decode_as_str_or_string(self.value)?;
        Ok(result)
    }

    pub fn parse<T: FromStr>(&'s self) -> Result<T, ReadingEncodedDataError> {
        // Decode first, then parse — otherwise a percent-encoded value (e.g. "%2D5"
        // for "-5") would be parsed from its raw, still-escaped form.
        let decoded = crate::url_decoder::decode_as_str_or_string(self.value)?;

        match decoded.as_str().parse::<T>() {
            Ok(value) => Ok(value),
            Err(_) => Err(ReadingEncodedDataError::CanNotParseValue(
                decoded.as_str().to_string(),
            )),
        }
    }
}
