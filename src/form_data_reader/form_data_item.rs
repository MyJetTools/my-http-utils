use rust_extensions::{slice_of_u8_utils::SliceOfU8Ext, MaybeShortString};

use crate::form_data_reader::ReadingFromDataError;

use super::ContentDispositionParser;

#[derive(Debug)]
pub enum FormDataItem<'s> {
    ValueAsString {
        name: &'s str,
        value: &'s str,
    },
    File {
        name: &'s str,
        file_name: &'s str,
        content_type: &'s str,
        content: &'s [u8],
    },
}

impl<'s> FormDataItem<'s> {
    pub fn unwrap_as_string(&'s self) -> Result<&'s str, ReadingFromDataError> {
        match self {
            FormDataItem::ValueAsString { value, .. } => Ok(value),
            FormDataItem::File { name, .. } => Err(ReadingFromDataError::ValidationError {
                field: name.to_string(),
                error: "Can not unwrap as string. It is file".to_string(),
            }),
        }
    }

    pub fn unwrap_as_file_name(&'s self) -> &'s str {
        match self {
            FormDataItem::ValueAsString { .. } => {
                panic!("Can not unwrap FormDataItem as file. It is a string value")
            }
            FormDataItem::File { file_name, .. } => file_name,
        }
    }

    pub fn get_name(&'s self) -> &'s str {
        match self {
            FormDataItem::ValueAsString { name, .. } => name,
            FormDataItem::File { name, .. } => name,
        }
    }

    /// Returns the raw content bytes — the file body, or the field value as bytes —
    /// without any JSON parsing.
    ///
    /// Use this instead of `TryInto<Vec<u8>>`: the blanket `TryInto<Vec<TValue>>` impl
    /// treats the content as JSON, so a `Vec<u8>` request would deserialize it as a JSON
    /// array of numbers rather than returning the bytes.
    pub fn as_bytes(&self) -> &'s [u8] {
        match self {
            FormDataItem::ValueAsString { value, .. } => value.as_bytes(),
            FormDataItem::File { content, .. } => content,
        }
    }

    /// Parses a single multipart part, panicking on malformed input.
    ///
    /// Kept for API compatibility — prefer [`Self::try_parse`], which reports errors
    /// instead of panicking. `FormDataReader` uses `try_parse` so that a malformed,
    /// attacker-controlled part is skipped rather than crashing the server.
    pub fn parse(src: &'s [u8]) -> Self {
        Self::try_parse(src).expect("FormDataItem::parse got malformed data")
    }

    /// Parses a single multipart part.
    ///
    /// Returns an error (rather than panicking) on any malformed/attacker-controlled
    /// input, so a bad part can be skipped instead of crashing the server.
    pub fn try_parse(src: &'s [u8]) -> Result<Self, ReadingFromDataError> {
        let mut content_type = None;
        let content;
        let mut file_name = None;
        let mut name = None;

        let mut pos = 0;

        loop {
            // Blank line (CRLFCRLF) marks the end of the part headers.
            match src.get(pos..pos + 4) {
                Some([13, 10, 13, 10]) => {
                    pos += 4;

                    let body = &src[pos..];
                    // Strip the trailing CRLF that precedes the next boundary, if present.
                    content = Some(if body.ends_with(&[13, 10]) {
                        &body[..body.len() - 2]
                    } else {
                        body
                    });
                    break;
                }
                Some(_) => {}
                None => {
                    return Err(parse_error(
                        "Unexpected end of multipart part: no blank line after headers",
                    ));
                }
            }

            pos = src
                .find_pos_by_condition(pos, |p| p > 32)
                .ok_or_else(|| parse_error("Malformed multipart part: no header start"))?;

            let colon_pos = src
                .find_byte_pos(b':', pos)
                .ok_or_else(|| parse_error("Malformed multipart header: missing ':'"))?;

            let header_name_str = std::str::from_utf8(&src[pos..colon_pos])
                .map_err(|_| parse_error("Malformed multipart header name (invalid utf8)"))?;

            let header_name = MaybeShortString::from_str_as_lower_case(header_name_str);
            let header_name = header_name.as_str();

            match header_name {
                "content-disposition" => {
                    let content_disposition_data = &src[colon_pos + 1..];

                    let end = content_disposition_data
                        .iter()
                        .position(|p| *p == 13)
                        .ok_or_else(|| parse_error("Malformed Content-Disposition: no CR"))?;

                    let content_disposition_data = &content_disposition_data[..end];

                    for itm in ContentDispositionParser::new(content_disposition_data) {
                        // MIME parameter names are case-insensitive.
                        match itm.key.to_ascii_lowercase().as_str() {
                            "name" => name = itm.value,
                            "filename" => file_name = itm.value,
                            _ => {}
                        }
                    }

                    pos = src
                        .find_byte_pos(13, colon_pos)
                        .ok_or_else(|| parse_error("Malformed Content-Disposition line"))?;
                }
                "content-type" => {
                    let start = src
                        .find_pos_by_condition(colon_pos + 1, |p| p > 32)
                        .ok_or_else(|| parse_error("Malformed Content-Type header"))?;
                    let end = src
                        .find_pos_by_condition(start, |p| p == 13)
                        .ok_or_else(|| parse_error("Malformed Content-Type line"))?;
                    content_type = Some(
                        std::str::from_utf8(&src[start..end])
                            .map_err(|_| parse_error("Content-Type is not valid utf8"))?,
                    );

                    pos = end;
                }
                _ => {
                    pos = src
                        .find_byte_pos(13, colon_pos)
                        .ok_or_else(|| parse_error("Malformed multipart header line"))?;
                }
            }
        }

        let content = content.ok_or_else(|| parse_error("Missing multipart content"))?;

        // A file part needs BOTH a Content-Type and a filename. Content-Type without a
        // filename (allowed by RFC 7578 on ordinary fields) is treated as a value.
        match (content_type, file_name) {
            (Some(content_type), Some(file_name)) => Ok(Self::File {
                file_name,
                content_type,
                content,
                name: name.ok_or_else(|| parse_error("File part is missing 'name'"))?,
            }),
            _ => Ok(Self::ValueAsString {
                value: if content.is_empty() {
                    ""
                } else {
                    std::str::from_utf8(content)
                        .map_err(|_| parse_error("Field value is not valid utf8"))?
                },
                name: name.ok_or_else(|| parse_error("Field is missing 'name'"))?,
            }),
        }
    }
}

fn parse_error(error: &str) -> ReadingFromDataError {
    ReadingFromDataError::ValidationError {
        field: "form-data".to_string(),
        error: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::form_data_reader::FormDataItem;

    #[test]
    fn test_value_as_string_parser() {
        let msg = vec![
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 100, 116, 70, 114, 111, 109, 34, 13, 10, 13, 10, 50, 13, 10,
        ];

        let item = super::FormDataItem::try_parse(&msg).unwrap();

        match item {
            FormDataItem::ValueAsString { value, name } => {
                assert_eq!(name, "dtFrom");
                assert_eq!(value, "2");
            }
            FormDataItem::File { .. } => {
                panic!("Should be value as string");
            }
        }
    }

    #[test]
    fn test_value_file_parser() {
        let msg = vec![
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 102, 105, 108, 101, 34, 59, 32, 102, 105, 108, 101, 110, 97, 109, 101, 61, 34, 116,
            101, 115, 116, 45, 112, 97, 121, 108, 111, 97, 100, 46, 116, 120, 116, 34, 13, 10, 67,
            111, 110, 116, 101, 110, 116, 45, 84, 121, 112, 101, 58, 32, 116, 101, 120, 116, 47,
            112, 108, 97, 105, 110, 13, 10, 13, 10, 49, 50, 51, 13, 10,
        ];

        println!("{:?}", std::str::from_utf8(msg.as_slice()));

        let item = super::FormDataItem::try_parse(&msg).unwrap();

        match item {
            FormDataItem::ValueAsString { value: _, name: _ } => {
                panic!("Should be value as string");
            }
            FormDataItem::File {
                name,
                file_name,
                content_type,
                content,
            } => {
                assert_eq!(name, "file");
                assert_eq!(file_name, "test-payload.txt");
                assert_eq!(content_type, "text/plain");
                assert_eq!(std::str::from_utf8(content).unwrap(), "123");
            }
        }
    }

    #[test]
    pub fn test_content_disposition_test_real_data() {
        let src: Vec<u8> = vec![
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 100, 111, 99, 73, 100, 34, 13, 10, 13, 10, 48, 13, 10,
        ];

        println!("src: {:?}", std::str::from_utf8(src.as_slice()).unwrap());

        let result = super::FormDataItem::try_parse(&src).unwrap();

        assert_eq!(result.get_name(), "docId");
        assert_eq!(result.unwrap_as_string().unwrap(), "0");
    }

    #[test]
    pub fn test_content() {
        let src = "Content-Disposition: form-data; name=\"file_title\"\n\ntest\n";

        let src = format_text_with_cl_cr(src);
        let result = super::FormDataItem::try_parse(src.as_slice()).unwrap();

        assert_eq!(result.get_name(), "file_title");
        assert_eq!(result.unwrap_as_string().unwrap(), "test");
    }

    #[test]
    pub fn test_content_with_empty_value() {
        let src = "Content-Disposition: form-data; name=\"file_title\"\n\n\n";

        let src = format_text_with_cl_cr(src);
        let result = super::FormDataItem::try_parse(src.as_slice()).unwrap();

        assert_eq!(result.get_name(), "file_title");
        assert_eq!(result.unwrap_as_string().unwrap(), "");
    }

    // Regression tests: none of these malformed inputs may panic (remote DoS vectors).
    #[test]
    fn test_short_part_does_not_panic() {
        assert!(super::FormDataItem::try_parse(b"").is_err());
        assert!(super::FormDataItem::try_parse(b"ab").is_err());
        assert!(super::FormDataItem::try_parse(&[13, 10, 13]).is_err());
    }

    #[test]
    fn test_missing_colon_does_not_panic() {
        let src = format_text_with_cl_cr("no-colon-header\n\nbody\n");
        assert!(super::FormDataItem::try_parse(src.as_slice()).is_err());
    }

    #[test]
    fn test_non_utf8_field_value_does_not_panic() {
        let mut src = format_text_with_cl_cr("Content-Disposition: form-data; name=\"a\"\n\n");
        src.push(0xFF); // invalid utf8 byte in the value
        src.extend_from_slice(&[13, 10]);
        assert!(super::FormDataItem::try_parse(src.as_slice()).is_err());
    }

    #[test]
    fn test_as_bytes_returns_raw_content() {
        // File content bytes are returned verbatim (not JSON-parsed).
        let file = FormDataItem::File {
            name: "f",
            file_name: "a.bin",
            content_type: "application/octet-stream",
            content: &[0u8, 1, 2, 255],
        };
        assert_eq!(file.as_bytes(), &[0u8, 1, 2, 255]);

        let value = FormDataItem::ValueAsString {
            name: "n",
            value: "hello",
        };
        assert_eq!(value.as_bytes(), b"hello");
    }

    #[test]
    fn test_content_type_without_filename_is_value() {
        // RFC 7578 allows Content-Type on ordinary fields; must not panic on the
        // missing filename and should read back as a value.
        let src = format_text_with_cl_cr(
            "Content-Disposition: form-data; name=\"note\"\nContent-Type: text/plain\n\nhello\n",
        );
        let item = super::FormDataItem::try_parse(src.as_slice()).unwrap();
        assert_eq!(item.get_name(), "note");
        assert_eq!(item.unwrap_as_string().unwrap(), "hello");
    }

    fn format_text_with_cl_cr(src: &str) -> Vec<u8> {
        let mut result = Vec::new();

        for c in src.as_bytes() {
            let c = *c;
            if c == b'\n' {
                result.push(13);
                result.push(10);
            } else {
                result.push(c);
            }
        }

        result
    }
}
