use crate::http_input::core::data_src::SRC_BODY_JSON;
use crate::http_input::HttpParseError;

use super::JsonEncodedValueAsString;

/// A JSON request body, viewed as a set of named members. It holds only the body byte slice and
/// resolves each member **lazily**, on demand, via `my_json`'s zero-copy path lookup — there is no
/// pre-parsed map and no per-member allocation (contrast the earlier
/// `HashMap<String, Box<serde_json::value::RawValue>>` reader). Each looked-up member keeps its
/// verbatim source text (see [`JsonEncodedValueAsString`]); `null` members report as absent.
pub struct JsonEncodedData<'s> {
    raw: &'s [u8],
}

impl<'s> JsonEncodedData<'s> {
    pub fn from_slice(raw: &'s [u8]) -> Result<Self, HttpParseError> {
        // Cheap up-front guard that the body is an object — named fields can only be read out of an
        // object (the whole-body `#[http_body_raw]` path does not go through this reader). Full
        // JSON validation is now lazy: a malformed member surfaces when that field is read, not
        // here, which is the deliberate trade for scanning without a pre-parse.
        for b in raw {
            if *b <= b' ' {
                continue;
            }
            if *b == b'{' {
                return Ok(Self { raw });
            }
            return Err(HttpParseError::InvalidBodyFormat(
                "JSON body must be an object to read named fields from it".to_string(),
            ));
        }

        Err(HttpParseError::InvalidBodyFormat(
            "JSON body is empty; can not read named fields from it".to_string(),
        ))
    }

    pub fn get_optional(&self, name: &'s str) -> Option<JsonEncodedValueAsString<'s>> {
        // `my_json` scans the body slice on demand and borrows the member's bytes (zero-copy). A
        // parse error is treated as "absent" here — the `Option` signature has no error channel;
        // for a required field this surfaces as a "field required" error via `get_required`.
        match my_json::j_path::get_value(self.raw, name) {
            Ok(Some(value)) if !value.is_null() => {
                // null members are dropped so `get_optional` reports them as absent.
                Some(JsonEncodedValueAsString::new(name, value))
            }
            _ => None,
        }
    }

    pub fn get_required(
        &self,
        name: &'s str,
    ) -> Result<JsonEncodedValueAsString<'s>, HttpParseError> {
        self.get_optional(name)
            .ok_or_else(|| HttpParseError::required(name, SRC_BODY_JSON))
    }
}
