use std::collections::HashMap;

use serde_json::value::RawValue;

use crate::http_input::data_src::SRC_BODY_JSON;
use crate::http_input::HttpParseError;

use super::JsonEncodedValueAsString;

/// A parsed JSON request body, viewed as a set of named members. Each member keeps its verbatim
/// source text (see [`JsonEncodedValueAsString`]); null members are dropped so `get_optional`
/// reports them as absent (matching the original reader).
pub struct JsonEncodedData<'s> {
    values: Vec<JsonEncodedValueAsString<'s>>,
}

impl<'s> JsonEncodedData<'s> {
    pub fn from_slice(raw: &[u8]) -> Result<Self, HttpParseError> {
        // Deserialize the top-level object into raw (un-parsed) members. A non-object body is an
        // error here — named body fields need an object (the whole-body `#[http_body_raw]` path
        // does not go through this reader).
        let object: HashMap<String, Box<RawValue>> = serde_json::from_slice(raw)
            .map_err(|err| HttpParseError::InvalidBodyFormat(format!("Can not parse Json body. {}", err)))?;

        let mut values = Vec::with_capacity(object.len());
        for (name, raw_value) in object {
            if raw_value.get() == "null" {
                continue;
            }
            values.push(JsonEncodedValueAsString::new(name, raw_value));
        }

        Ok(Self { values })
    }

    pub fn get_optional(&'s self, name: &str) -> Option<&'s JsonEncodedValueAsString<'s>> {
        self.values.iter().find(|value| value.get_name() == name)
    }

    pub fn get_required(
        &'s self,
        name: &str,
    ) -> Result<&'s JsonEncodedValueAsString<'s>, HttpParseError> {
        self.get_optional(name)
            .ok_or_else(|| HttpParseError::required(name, SRC_BODY_JSON))
    }
}
