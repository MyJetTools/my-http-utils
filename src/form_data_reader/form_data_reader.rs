use crate::form_data_reader::{FormDataItem, ReadingFromDataError};

use super::content_iterator::ContentIterator;

pub struct FormDataReader<'s> {
    data: Vec<FormDataItem<'s>>,
}

impl<'s> FormDataReader<'s> {
    // `boundary` gets an independent lifetime: it is only read while splitting the content
    // (and copied into an owned `ShortString` inside `ContentIterator`), never retained in a
    // `FormDataItem`. This lets callers pass a short-lived boundary (e.g. one owned by a
    // `BodyContentType`) while the reader keeps borrowing only `content`.
    pub fn new<'b>(content: &'s [u8], boundary: &'b str) -> Self {
        let mut data = Vec::new();

        for chunk in ContentIterator::new(content, boundary) {
            // Skip malformed parts rather than panicking on attacker-controlled input.
            if let Ok(item) = FormDataItem::try_parse(chunk) {
                data.push(item);
            }
        }

        Self { data }
    }
    pub fn get_required(
        &'s self,
        name: &str,
    ) -> Result<&'s FormDataItem<'s>, ReadingFromDataError> {
        for itm in &self.data {
            if itm.get_name() == name {
                return Ok(itm);
            }
        }

        Err(ReadingFromDataError::ParameterMissing(name.to_string()))
    }

    pub fn get_optional(&'s self, name: &str) -> Option<&'s FormDataItem<'s>> {
        self.data.iter().find(|&itm| itm.get_name() == name).map(|v| v as _)
    }
}

#[cfg(test)]
mod tests {
    use super::FormDataReader;

    #[test]
    fn test() {
        let payload = std::include_bytes!("../../test_form_data_payload.txt");

        let payload = format_text_with_cl_cr(payload);

        let form_data_reader = FormDataReader::new(
            payload.as_slice(),
            "------DataFormBoundary15c4050a1c8749f2a53abd41b27c9d0f",
        );

        // The payload must parse into at least one addressable item, and lookups must work.
        assert!(!form_data_reader.data.is_empty());
        let first_name = form_data_reader.data[0].get_name().to_string();
        assert!(form_data_reader.get_required(&first_name).is_ok());
        assert!(form_data_reader.get_optional("____definitely_missing____").is_none());
    }

    #[test]
    fn test_malformed_payload_does_not_panic() {
        // A body full of boundary noise must not crash the reader.
        let reader = FormDataReader::new(b"garbage without any boundary at all", "----b");
        assert!(reader.get_optional("anything").is_none());
    }

    fn format_text_with_cl_cr(src: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();

        for c in src {
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
