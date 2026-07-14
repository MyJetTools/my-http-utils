use rust_extensions::StrOrString;

use super::{UrlDecodeError, UrlDecoder};

pub fn decode_from_url_query_string(src: &str) -> Result<String, UrlDecodeError> {
    if !has_escape(src.as_bytes()) {
        return Ok(src.to_string());
    }

    let mut result: Vec<u8> = Vec::with_capacity(src.len());
    let mut url_decoder = UrlDecoder::new(src);

    while let Some(next_one) = url_decoder.get_next()? {
        result.push(next_one);
    }

    Ok(String::from_utf8(result)?)
}

pub fn decode_as_str_or_string<'s>(src: &'s str) -> Result<StrOrString<'s>, UrlDecodeError> {
    if !has_escape(src.as_bytes()) {
        return Ok(StrOrString::create_as_str(src));
    }

    let mut result: Vec<u8> = Vec::with_capacity(src.len());
    let mut url_decoder = UrlDecoder::new(src);

    while let Some(next_one) = url_decoder.get_next()? {
        result.push(next_one);
    }

    Ok(StrOrString::create_as_string(String::from_utf8(result)?))
}

fn has_escape(src: &[u8]) -> bool {
    for itm in src {
        if *itm == b'%' {
            return true;
        }

        if *itm == b'+' {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_url_decoding() {
        let value = "http%3A%2F%2F127.0.0.1%3A5223";

        let result = super::decode_from_url_query_string(value);

        assert_eq!("http://127.0.0.1:5223", result.unwrap().as_str());
    }

    #[test]
    fn test_url_decoding_case_2() {
        let value = "Euro%20Stoxx%2050";

        let result = super::decode_from_url_query_string(value);

        assert_eq!("Euro Stoxx 50", result.unwrap().as_str());
    }
    #[test]
    fn test_incomplete_escape_errors() {
        let value = "abc%";

        let result = super::decode_from_url_query_string(value);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .msg
            .contains("Unexpected end of input after '%' escape"));
    }

    #[test]
    fn test_invalid_hex_errors() {
        let value = "abc%2Z";

        let result = super::decode_from_url_query_string(value);

        assert!(result.is_err());
        assert!(result.unwrap_err().msg.contains("Invalid escape char"));
    }

    #[test]
    fn test_invalid_utf8_errors() {
        // %C3%28 decodes to bytes [0xC3, 0x28], which is not valid UTF-8
        let value = "%C3%28";

        let result = super::decode_from_url_query_string(value);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .msg
            .contains("Can not decode Utf8 string"));
    }

    #[test]
    fn test_url_decoding_case_3() {
        let value = "value1%7Cvalue2";

        let result = super::decode_from_url_query_string(value);

        assert_eq!("value1|value2", result.unwrap().as_str());
    }

    #[test]
    fn test_lowercase_hex() {
        let value = "%d0%9f";
        let result = super::decode_from_url_query_string(value);
        assert_eq!("П", result.unwrap().as_str());
    }

    #[test]
    fn test_str_or_string_decodes_non_ascii() {
        // Regression: this used to push each byte `as char`, producing mojibake.
        let value = "%D0%9F%D1%80%D0%B8%D0%B2%D0%B5%D1%82";
        let result = super::decode_as_str_or_string(value).unwrap();
        assert_eq!("Привет", result.as_str());
    }

    #[test]
    fn test_str_or_string_plus_is_space() {
        let result = super::decode_as_str_or_string("a+b+c").unwrap();
        assert_eq!("a b c", result.as_str());
    }

    #[test]
    fn test_str_or_string_borrows_when_no_escape() {
        let result = super::decode_as_str_or_string("plain").unwrap();
        assert_eq!("plain", result.as_str());
    }

    #[test]
    fn test_str_or_string_invalid_utf8_errors() {
        let result = super::decode_as_str_or_string("%C3%28");
        assert!(result.is_err());
    }
}
