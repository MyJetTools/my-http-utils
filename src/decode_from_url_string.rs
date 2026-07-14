use rust_extensions::StrOrString;

/// Decodes a percent-encoded string.
///
/// Thin, infallible wrapper over [`crate::url_decoder::decode_as_str_or_string`], kept for
/// API compatibility with existing consumers. On a malformed escape (truncated `%`, invalid
/// hex, or invalid UTF-8) it returns the input unchanged instead of panicking.
pub fn decode_from_url_string<'s>(src: &'s str) -> StrOrString<'s> {
    match crate::url_decoder::decode_as_str_or_string(src) {
        Ok(result) => result,
        Err(_) => StrOrString::create_as_str(src),
    }
}

/// Percent-encodes `src` and appends the result to `res`.
///
/// Shares the single encoder implementation in [`crate::url_encoder`].
pub fn encode_to_url_string_and_copy(res: &mut String, src: &str) {
    for c in src.chars() {
        crate::url_encoder::encode_char_and_copy(res, c);
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_from_real_life() {
        let src = "4%2F0AeaYSHA_pv6LYFSy9QdDASiSdr4X53iOaoo9ZJotKi536ELdyaLNqbsaQ0sjsTE9yuhhdQ";

        let result = super::decode_from_url_string(src);

        assert_eq!(
            result.as_str(),
            "4/0AeaYSHA_pv6LYFSy9QdDASiSdr4X53iOaoo9ZJotKi536ELdyaLNqbsaQ0sjsTE9yuhhdQ"
        );
    }

    #[test]
    fn test_space_escape_no_longer_panics() {
        // %20 used to panic in the old hand-written table.
        let result = super::decode_from_url_string("Euro%20Stoxx%2050");
        assert_eq!(result.as_str(), "Euro Stoxx 50");
    }

    #[test]
    fn test_non_ascii_utf8() {
        let result = super::decode_from_url_string("%D0%9F%D1%80%D0%B8%D0%B2%D0%B5%D1%82");
        assert_eq!(result.as_str(), "Привет");
    }

    #[test]
    fn test_malformed_input_returns_source() {
        // Truncated escape must not panic; best-effort passthrough.
        assert_eq!(super::decode_from_url_string("abc%2").as_str(), "abc%2");
        assert_eq!(super::decode_from_url_string("abc%").as_str(), "abc%");
    }

    #[test]
    fn test_encode_to_url_string_and_copy() {
        let mut res = String::new();
        super::encode_to_url_string_and_copy(&mut res, "a|b c");
        assert_eq!(res, "a%7Cb+c");
    }
}
