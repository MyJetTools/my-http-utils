#[derive(Debug)]
pub struct KeyValue<'s> {
    pub key: &'s str,
    pub value: Option<&'s str>,
}

pub struct ContentDispositionParser<'s> {
    content: &'s [u8],
    pos: usize,
}

impl<'s> ContentDispositionParser<'s> {
    pub fn new(content: &'s [u8]) -> Self {
        Self { content, pos: 0 }
    }
    pub fn find_pos<TCondition: Fn(u8) -> bool>(&self, condition: TCondition) -> Option<usize> {
        self.find_from(self.pos, condition)
    }

    fn find_from<TCondition: Fn(u8) -> bool>(
        &self,
        from: usize,
        condition: TCondition,
    ) -> Option<usize> {
        (from..self.content.len()).find(|&i| condition(self.content[i]))
    }
}

impl<'s> Iterator for ContentDispositionParser<'s> {
    type Item = KeyValue<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        // Skip leading whitespace and any ';' separator left over from the previous pair.
        self.pos = self.find_pos(|b| b > 32 && b != b';')?;

        let pos = self.find_pos(|b| b == b';' || b == b'=')?;

        let b = self.content[pos];

        // Invalid utf8 ends iteration rather than panicking.
        let key = std::str::from_utf8(&self.content[self.pos..pos]).ok()?;

        self.pos = pos + 1;

        if b == b';' {
            return Some(KeyValue { key, value: None });
        }

        // After '=' the value is either a quoted string ("...") or a bare token.
        let value_start = self.find_pos(|b| b > 32)?;

        if self.content[value_start] == b'"' {
            // Quoted value: find the closing quote, skipping backslash-escaped quotes
            // so that `filename="a\"b.txt"` is not truncated at the escaped quote.
            let mut i = value_start + 1;
            let mut escaped = false;
            let end = loop {
                if i >= self.content.len() {
                    break i; // unterminated quote — take the rest
                }
                let c = self.content[i];
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    break i;
                }
                i += 1;
            };

            let value = std::str::from_utf8(&self.content[value_start + 1..end]).ok()?;
            self.pos = (end + 1).min(self.content.len());
            Some(KeyValue {
                key,
                value: Some(value),
            })
        } else {
            // Bare (unquoted) token value: up to the next ';' or whitespace.
            let end = self
                .find_from(value_start, |b| b == b';' || b <= 32)
                .unwrap_or(self.content.len());

            let value = std::str::from_utf8(&self.content[value_start..end]).ok()?;
            self.pos = end;
            Some(KeyValue {
                key,
                value: Some(value),
            })
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    pub fn test_content_disposition_as_field() {
        let src: Vec<u8> = vec![
            32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61, 34, 100,
            116, 70, 114, 111, 109, 34, 13, 10,
        ];

        let result = ContentDispositionParser::new(&src).collect::<Vec<_>>();

        {
            let first_item = result.get(0).unwrap();
            assert_eq!(first_item.key, "form-data");
            assert_eq!(first_item.value, None);
        }

        {
            let first_item = result.get(1).unwrap();
            assert_eq!(first_item.key, "name");
            assert_eq!(first_item.value, Some("dtFrom"));
        }
    }

    #[test]
    pub fn test_content_disposition_as_file() {
        let src: Vec<u8> = vec![
            32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61, 34, 102,
            105, 108, 101, 34, 59, 32, 102, 105, 108, 101, 110, 97, 109, 101, 61, 34, 116, 101,
            115, 116, 45, 112, 97, 121, 108, 111, 97, 100, 46, 116, 120, 116, 34, 13, 10, 67, 111,
            110, 116, 101, 110, 116, 45, 84, 121, 112, 101, 58, 32, 116, 101, 120, 116, 47, 112,
            108, 97, 105, 110, 13, 10, 13, 10, 49, 50, 51, 13, 10,
        ];

        let result = ContentDispositionParser::new(&src).collect::<Vec<_>>();

        {
            let first_item = result.get(0).unwrap();
            assert_eq!(first_item.key, "form-data");
            assert_eq!(first_item.value, None);
        }

        {
            let first_item = result.get(1).unwrap();
            assert_eq!(first_item.key, "name");
            assert_eq!(first_item.value, Some("file"));
        }

        {
            let first_item = result.get(2).unwrap();
            assert_eq!(first_item.key, "filename");
            assert_eq!(first_item.value, Some("test-payload.txt"));
        }
    }

    #[test]
    pub fn test_unquoted_value() {
        // Regression: unquoted token values used to be dropped (then panic downstream).
        let result = ContentDispositionParser::new(b"form-data; name=file; x=1")
            .collect::<Vec<_>>();
        assert_eq!(result[0].key, "form-data");
        assert_eq!(result[1].key, "name");
        assert_eq!(result[1].value, Some("file"));
        assert_eq!(result[2].key, "x");
        assert_eq!(result[2].value, Some("1"));
    }

    #[test]
    pub fn test_escaped_quote_in_value_is_not_truncated() {
        // Regression: `filename="a\"b.txt"` used to truncate at the escaped quote.
        let result =
            ContentDispositionParser::new(b"form-data; filename=\"a\\\"b.txt\"").collect::<Vec<_>>();
        let filename = result.iter().find(|kv| kv.key == "filename").unwrap();
        assert_eq!(filename.value, Some("a\\\"b.txt"));
    }
}
