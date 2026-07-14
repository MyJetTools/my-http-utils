use serde_json::Value;

use crate::http_input::data_src::SRC_BODY_JSON;
use crate::http_input::HttpParseError;

use super::JsonEncodedValueAsString;

/// A parsed JSON request body, viewed as a set of named members. Null members are dropped so
/// that `get_optional` reports them as absent (matching the original `my-json`-based reader).
pub struct JsonEncodedData<'s> {
    values: Vec<JsonEncodedValueAsString<'s>>,
}

impl<'s> JsonEncodedData<'s> {
    pub fn from_slice(raw: &[u8]) -> Result<Self, HttpParseError> {
        let root: Value = serde_json::from_slice(raw)
            .map_err(|err| HttpParseError::InvalidBodyFormat(format!("Can not parse Json body. {}", err)))?;

        let object = match root {
            Value::Object(map) => map,
            _ => {
                return Err(HttpParseError::InvalidBodyFormat(
                    "Json body is expected to be an object".to_string(),
                ))
            }
        };

        let mut values = Vec::with_capacity(object.len());
        for (name, value) in object {
            if value.is_null() {
                continue;
            }
            values.push(JsonEncodedValueAsString::new(name, value));
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
