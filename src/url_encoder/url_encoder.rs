use rust_extensions::StrOrString;

pub fn encode_string<'s>(src: &'s str) -> StrOrString<'s> {
    if !has_to_be_encoded(src) {
        return StrOrString::create_as_str(src);
    }

    let mut result = String::with_capacity(src.len());

    for c in src.chars() {
        encode_char_and_copy(&mut result, c);
    }

    StrOrString::create_as_string(result)
}

/// Percent-encodes a single char into `result`.
///
/// * ASCII symbols listed in `URL_ENCODE_SYMBOLS` are replaced with their escape.
/// * ASCII control characters (and DEL) are percent-encoded byte-wise.
/// * Non-ASCII characters are percent-encoded byte-wise over their UTF-8 bytes.
/// * Everything else is copied verbatim.
///
/// Space follows the `x-www-form-urlencoded` convention (`+`), matching the
/// decoder in `crate::url_decoder`.
pub fn encode_char_and_copy(result: &mut String, c: char) {
    if c.is_ascii() {
        if let Some(escaped) = super::encode_map::URL_ENCODE_SYMBOLS.get(&c) {
            result.push_str(escaped);
        } else if c.is_ascii_control() {
            push_percent_byte(result, c as u8);
        } else {
            result.push(c);
        }
    } else {
        let mut buf = [0u8; 4];
        for b in c.encode_utf8(&mut buf).as_bytes() {
            push_percent_byte(result, *b);
        }
    }
}

/// Percent-encodes a single path segment into `result`.
///
/// Uses **path** rules (RFC 3986 `pchar`), which differ from the query encoder:
/// space becomes `%20` (not `+`), and the path separator `/` is kept verbatim.
/// The unreserved set, sub-delims, and `:` `@` are kept; everything else — space,
/// control chars, `?`, `#`, `%`, other gen-delims, and non-ASCII (encoded byte-wise
/// as UTF-8) — is percent-encoded.
pub fn encode_path_segment_and_copy(result: &mut String, segment: &str) {
    for c in segment.chars() {
        if is_path_segment_safe(c) {
            result.push(c);
        } else if c.is_ascii() {
            push_percent_byte(result, c as u8);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).as_bytes() {
                push_percent_byte(result, *b);
            }
        }
    }
}

/// `pchar` (RFC 3986) plus the `/` separator, which `append_path_segment` keeps.
fn is_path_segment_safe(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            // unreserved
            '-' | '.' | '_' | '~'
            // sub-delims
            | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '='
            // pchar extras + path separator
            | ':' | '@' | '/'
        )
}

fn push_percent_byte(result: &mut String, b: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    result.push('%');
    result.push(HEX[(b >> 4) as usize] as char);
    result.push(HEX[(b & 0x0F) as usize] as char);
}

fn has_to_be_encoded(src: &str) -> bool {
    for c in src.chars() {
        if !c.is_ascii() || c.is_ascii_control() {
            return true;
        }

        if super::encode_map::URL_ENCODE_SYMBOLS.contains_key(&c) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod test_encodes {

    #[test]
    fn test() {
        let value = "value1|value2";

        let result = super::encode_string(value);

        assert_eq!("value1%7Cvalue2", result.as_str());
    }

    #[test]
    fn test_non_ascii_is_percent_encoded() {
        // Cyrillic "Пример" — each char is a 2-byte UTF-8 sequence.
        let result = super::encode_string("Пример");
        assert_eq!("%D0%9F%D1%80%D0%B8%D0%BC%D0%B5%D1%80", result.as_str());
    }

    #[test]
    fn test_carriage_return_is_percent_zero_d() {
        let result = super::encode_string("a\rb");
        assert_eq!("a%0Db", result.as_str());
    }

    #[test]
    fn test_no_encoding_needed_borrows() {
        let result = super::encode_string("plain-value_123");
        assert_eq!("plain-value_123", result.as_str());
    }

    #[test]
    fn test_round_trip_with_decoder() {
        let src = "Hello, Мир! 100% & more <tag>";
        let encoded = super::encode_string(src);
        let decoded =
            crate::url_decoder::decode_from_url_query_string(encoded.as_str()).unwrap();
        assert_eq!(src, decoded.as_str());
    }

    fn encode_path(segment: &str) -> String {
        let mut out = String::new();
        super::encode_path_segment_and_copy(&mut out, segment);
        out
    }

    #[test]
    fn test_path_space_is_percent_20_not_plus() {
        assert_eq!(encode_path("a b"), "a%20b");
    }

    #[test]
    fn test_path_non_ascii() {
        assert_eq!(encode_path("café"), "caf%C3%A9");
        assert_eq!(encode_path("ц"), "%D1%86");
    }

    #[test]
    fn test_path_structural_reserved_are_encoded() {
        assert_eq!(encode_path("a%b"), "a%25b");
        assert_eq!(encode_path("a?b"), "a%3Fb");
        assert_eq!(encode_path("a#b"), "a%23b");
    }

    #[test]
    fn test_path_plain_ascii_is_unchanged() {
        // pchar (unreserved + sub-delims + ':' '@') and '/' must pass through verbatim.
        for s in [
            "templates",
            "api",
            "user-name_1.2~x",
            "a/b/c",
            "user@host",
            "id:42",
            "a+b,c;d=e!f$g&h'i(j)k*l",
        ] {
            assert_eq!(encode_path(s), s, "segment must be unchanged: {s}");
        }
    }

    #[test]
    fn test_path_control_chars_are_encoded() {
        assert_eq!(encode_path("a\tb"), "a%09b");
        assert_eq!(encode_path("a\nb"), "a%0Ab");
    }
}
