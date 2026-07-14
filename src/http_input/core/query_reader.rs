use crate::url_encoded_data_reader::UrlEncodedDataReader;

use crate::http_input::{HttpInputValue, HttpParseError};

use super::data_src::SRC_QUERY_STRING;

/// Reads model fields out of a query string. Thin wrapper over [`UrlEncodedDataReader`] that
/// tags every value with `SRC_QUERY_STRING` and lifts it into an [`HttpInputValue`]. Built by
/// the derive-generated `parse` from `request.get_query_string()`.
pub struct QueryStringReader<'s> {
    reader: UrlEncodedDataReader<'s>,
}

impl<'s> QueryStringReader<'s> {
    pub fn new(query_string: &'s str) -> Result<Self, HttpParseError> {
        Ok(Self {
            reader: UrlEncodedDataReader::new(query_string)?,
        })
    }

    pub fn get_optional(&'s self, name: &str) -> Option<HttpInputValue<'s>> {
        self.reader
            .get_optional(name)
            .map(|value| HttpInputValue::from_url_encoded(value, SRC_QUERY_STRING))
    }

    pub fn get_required(&'s self, name: &str) -> Result<HttpInputValue<'s>, HttpParseError> {
        self.get_optional(name)
            .ok_or_else(|| HttpParseError::required(name, SRC_QUERY_STRING))
    }

    /// All values for a repeated `name` (`name[]=1&name[]=2` → two items). Empty vec if none.
    pub fn get_vec(&'s self, name: &str) -> Result<Vec<HttpInputValue<'s>>, HttpParseError> {
        Ok(self
            .reader
            .get_vec(name)
            .into_iter()
            .map(|value| HttpInputValue::from_url_encoded(value, SRC_QUERY_STRING))
            .collect())
    }
}
