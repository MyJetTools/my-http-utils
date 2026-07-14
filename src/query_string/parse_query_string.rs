use crate::{url_decoder::UrlDecodeError, url_encoded_data_reader::UrlEncodedValue};

/// Parses an `application/x-www-form-urlencoded` string into decoded-key values.
///
/// This is the single query-string parser shared by `UrlEncodedDataReader`.
///
/// * Keys are percent-decoded; values are kept raw (decoded on demand by `UrlEncodedValue`).
/// * Flag-style parameters without `=` (e.g. `debug&a=1`) are preserved with an empty value.
/// * Empty segments (from `&&` or a trailing `&`) are skipped.
pub fn parse<'s>(query_string: &'s str) -> Result<Vec<UrlEncodedValue<'s>>, UrlDecodeError> {
    let mut result = Vec::new();

    for el in query_string.split('&') {
        if el.is_empty() {
            continue;
        }

        match el.find('=') {
            Some(index) => {
                let key = crate::url_decoder::decode_from_url_query_string(&el[..index])?;
                result.push(UrlEncodedValue::new(key, &el[index + 1..]));
            }
            None => {
                let key = crate::url_decoder::decode_from_url_query_string(el)?;
                result.push(UrlEncodedValue::new(key, ""));
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_basic() {
        let items = super::parse("a=1&b=hello+world").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get_name(), "a");
        assert_eq!(items[1].as_string().unwrap(), "hello world");
    }

    #[test]
    fn test_flag_param_is_preserved() {
        let items = super::parse("debug&a=1").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get_name(), "debug");
        assert_eq!(items[0].as_string().unwrap(), "");
    }

    #[test]
    fn test_empty_segments_skipped() {
        let items = super::parse("a=1&&b=2&").unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_decoded_key() {
        let items = super::parse("a%20b=1").unwrap();
        assert_eq!(items[0].get_name(), "a b");
    }
}
