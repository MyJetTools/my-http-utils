use std::collections::HashMap;

use crate::url_decoder::UrlDecodeError;

pub fn parse_query_string(query_string: &str) -> Result<HashMap<String, String>, UrlDecodeError> {
    let mut result = HashMap::new();
    let elements = query_string.split("&");

    for el in elements {
        let kv = el.find('=');

        if let Some(index) = kv {
            let key = super::url_decoder::decode_from_url_string(&el[..index])?;
            let value = super::url_decoder::decode_from_url_string(&el[index + 1..])?;
            result.insert(key, value);
        }
    }

    Ok(result)
}
