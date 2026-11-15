use super::{ContentDispositionParser, ReadingFromDataError};

/// The header of one `multipart/form-data` part. It is the same for every part (RFC 7578): the
/// `name` of the field the part fills, and — when the part carries a file — the file's `filename`
/// and the part's `Content-Type`.
///
/// It is parsed as it is, with no opinion on what the part *is*. Whether a `filename` makes it a
/// file, or a missing `Content-Type` makes it text, is up to the type the part is read into — see
/// [`crate::http_input::FormDataProperty`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormDataHeader<'s> {
    /// `Content-Disposition: form-data; name="…"` — the field this part fills.
    pub name: &'s str,
    /// `Content-Disposition: …; filename="…"` — present on a file part.
    pub file_name: Option<&'s str>,
    /// The part's `Content-Type`. Optional: RFC 7578 reads a part without one as `text/plain`.
    pub content_type: Option<&'s str>,
}

impl<'s> FormDataHeader<'s> {
    /// A header that carries a name and nothing else — what a value comes with when it is read out
    /// of a body that has no part headers (`x-www-form-urlencoded`, JSON).
    pub fn new(name: &'s str) -> Self {
        Self {
            name,
            file_name: None,
            content_type: None,
        }
    }
}

/// One part of a `multipart/form-data` body: its [`FormDataHeader`], and its content byte for byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormDataPart<'s> {
    pub header: FormDataHeader<'s>,
    pub content: &'s [u8],
}

impl<'s> FormDataPart<'s> {
    /// Parses one part: the bytes between two boundary lines, headers first.
    ///
    /// The content is not looked at — any bytes are a valid content. An error (never a panic, the
    /// input is attacker-controlled) means the headers themselves are malformed: no blank line
    /// after them, a header line without `:`, a non-UTF-8 `Content-Type`, or no `name`.
    pub fn try_parse(src: &'s [u8]) -> Result<Self, ReadingFromDataError> {
        let mut name = None;
        let mut file_name = None;
        let mut content_type = None;

        let mut pos = 0;

        let content = loop {
            let line_end = find_crlf(src, pos).ok_or_else(|| {
                parse_error("Unexpected end of multipart part: no blank line after headers")
            })?;

            if line_end == pos {
                // The blank line: the headers are over, the content follows it.
                let content = &src[pos + 2..];
                // The CRLF in front of the next boundary belongs to the delimiter, not the content.
                break content.strip_suffix(b"\r\n").unwrap_or(content);
            }

            let line = &src[pos..line_end];
            pos = line_end + 2;

            let colon = line
                .iter()
                .position(|b| *b == b':')
                .ok_or_else(|| parse_error("Malformed multipart header: missing ':'"))?;

            let header_name = trim(&line[..colon]);
            let value = trim(&line[colon + 1..]);

            if header_name.eq_ignore_ascii_case(b"content-disposition") {
                for itm in ContentDispositionParser::new(value) {
                    // MIME parameter names are case-insensitive.
                    if itm.key.eq_ignore_ascii_case("name") {
                        name = itm.value;
                    } else if itm.key.eq_ignore_ascii_case("filename") {
                        file_name = itm.value;
                    }
                }
            } else if header_name.eq_ignore_ascii_case(b"content-type") && !value.is_empty() {
                // Read within its own line: an empty value is no Content-Type, not the start of
                // whatever comes next.
                content_type = Some(
                    std::str::from_utf8(value)
                        .map_err(|_| parse_error("Content-Type is not valid utf8"))?,
                );
            }
        };

        let name = name.ok_or_else(|| parse_error("Multipart part is missing 'name'"))?;

        Ok(Self {
            header: FormDataHeader {
                name,
                file_name,
                content_type,
            },
            content,
        })
    }
}

fn find_crlf(src: &[u8], from: usize) -> Option<usize> {
    src.get(from..)?
        .windows(2)
        .position(|pair| pair == b"\r\n")
        .map(|pos| from + pos)
}

/// Header values are padded with spaces / tabs only; a CR or LF never gets this far.
fn trim(src: &[u8]) -> &[u8] {
    let is_padding = |b: &u8| *b == b' ' || *b == b'\t';
    let start = src.iter().position(|b| !is_padding(b)).unwrap_or(src.len());
    let end = src
        .iter()
        .rposition(|b| !is_padding(b))
        .map_or(start, |pos| pos + 1);
    &src[start..end]
}

pub(super) fn parse_error(error: &str) -> ReadingFromDataError {
    ReadingFromDataError::ValidationError {
        field: "form-data".to_string(),
        error: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{FormDataHeader, FormDataPart};

    fn crlf(src: &str) -> Vec<u8> {
        src.replace('\n', "\r\n").into_bytes()
    }

    #[test]
    fn a_value_part_has_a_name_only() {
        let src = crlf("Content-Disposition: form-data; name=\"title\"\n\nhello\n");
        let part = FormDataPart::try_parse(&src).unwrap();
        assert_eq!(part.header, FormDataHeader::new("title"));
        assert_eq!(part.content, b"hello");
    }

    #[test]
    fn a_file_part_has_its_file_name_and_content_type() {
        let src = crlf(
            "Content-Disposition: form-data; name=\"cert\"; filename=\"ca.pem\"\nContent-Type: application/x-pem-file\n\nPEM\n",
        );
        let part = FormDataPart::try_parse(&src).unwrap();
        assert_eq!(part.header.name, "cert");
        assert_eq!(part.header.file_name, Some("ca.pem"));
        assert_eq!(part.header.content_type, Some("application/x-pem-file"));
        assert_eq!(part.content, b"PEM");
    }

    #[test]
    fn the_content_is_kept_byte_for_byte() {
        let mut src = crlf("Content-Disposition: form-data; name=\"bin\"\n\n");
        let content = [
            b'\r', b'\n', 0, 0xff, b'\r', b'\n', b'-', b'-', b'\r', b'\n',
        ];
        src.extend_from_slice(&content);
        src.extend_from_slice(b"\r\n");

        let part = FormDataPart::try_parse(&src).unwrap();
        assert_eq!(part.content, content);
    }

    #[test]
    fn header_names_and_parameters_are_case_insensitive() {
        let src = crlf("content-DISPOSITION: form-data; NAME=\"a\"; FileName=\"b.txt\"\nCONTENT-type:text/plain\n\nx\n");
        let part = FormDataPart::try_parse(&src).unwrap();
        assert_eq!(part.header.name, "a");
        assert_eq!(part.header.file_name, Some("b.txt"));
        assert_eq!(part.header.content_type, Some("text/plain"));
    }

    #[test]
    fn an_empty_content_type_does_not_reach_into_the_content() {
        let src = crlf(
            "Content-Disposition: form-data; name=\"f\"; filename=\"a.bin\"\nContent-Type: \n\nabc\n",
        );
        let part = FormDataPart::try_parse(&src).unwrap();
        assert_eq!(part.header.content_type, None);
        assert_eq!(part.header.file_name, Some("a.bin"));
        assert_eq!(part.content, b"abc");
    }

    #[test]
    fn a_file_name_without_a_content_type_is_kept() {
        let src = crlf("Content-Disposition: form-data; name=\"f\"; filename=\"a.txt\"\n\nabc\n");
        let part = FormDataPart::try_parse(&src).unwrap();
        assert_eq!(part.header.file_name, Some("a.txt"));
        assert_eq!(part.header.content_type, None);
    }

    #[test]
    fn unknown_headers_are_skipped() {
        let src = crlf(
            "Content-Disposition: form-data; name=\"f\"\nContent-Transfer-Encoding: binary\n\nabc\n",
        );
        let part = FormDataPart::try_parse(&src).unwrap();
        assert_eq!(part.header, FormDataHeader::new("f"));
        assert_eq!(part.content, b"abc");
    }

    #[test]
    fn malformed_parts_are_errors_not_panics() {
        assert!(FormDataPart::try_parse(b"").is_err());
        assert!(FormDataPart::try_parse(b"ab").is_err());
        assert!(FormDataPart::try_parse(&[13, 10, 13]).is_err());
        assert!(FormDataPart::try_parse(&crlf("no-colon-header\n\nbody\n")).is_err());
        // No `name`.
        assert!(
            FormDataPart::try_parse(&crlf("Content-Disposition: form-data\n\nbody\n")).is_err()
        );
        // No blank line after the headers.
        assert!(
            FormDataPart::try_parse(&crlf("Content-Disposition: form-data; name=\"a\"\n")).is_err()
        );
    }
}
