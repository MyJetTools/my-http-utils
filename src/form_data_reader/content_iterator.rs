use rust_extensions::{slice_of_u8_utils::SliceOfU8Ext, ShortString};

pub struct ContentIterator<'s> {
    boundary_data: ShortString,
    payload: &'s [u8],
    pos: usize,
}

impl<'s> ContentIterator<'s> {
    pub fn new(payload: &'s [u8], boundary: &str) -> Self {
        let mut boundary_data = ShortString::new_empty();
        boundary_data.push_str("--");
        boundary_data.push_str(boundary);

        Self {
            boundary_data,
            payload,
            pos: 0,
        }
    }
}

impl<'s> Iterator for ContentIterator<'s> {
    type Item = &'s [u8];

    fn next(&mut self) -> Option<Self::Item> {
        // Locate the boundary that opens the next part (this also skips any preamble
        // before the first boundary, instead of assuming the payload starts with one).
        let boundary_pos = self
            .payload
            .find_sequence_pos(self.boundary_data.as_bytes(), self.pos)?;

        let after_boundary = boundary_pos + self.boundary_data.len();

        // Closing delimiter "--boundary--" ends the stream; a missing next boundary
        // (truncated body / epilogue) also stops us rather than yielding garbage.
        if self.payload.get(after_boundary) == Some(&b'-')
            && self.payload.get(after_boundary + 1) == Some(&b'-')
        {
            return None;
        }

        // Skip the CRLF (or any whitespace) that follows the boundary line.
        let content_start = find_non_space(self.payload, after_boundary)?;

        let next_pos = self
            .payload
            .find_sequence_pos(self.boundary_data.as_bytes(), content_start)?;

        let result = &self.payload[content_start..next_pos];

        self.pos = next_pos;
        Some(result)
    }
}

fn find_non_space(payload: &[u8], pos_from: usize) -> Option<usize> {
    (pos_from..payload.len()).find(|&i| payload[i] > 32)
}

#[cfg(test)]
mod tests {

    use crate::form_data_reader::FormDataItem;

    use super::ContentIterator;

    #[test]
    fn test_splitting() {
        let boundary = "----WebKitFormBoundaryu7oxE5T3UC2xY2Q9";

        let payload: Vec<u8> = vec![
            45, 45, 45, 45, 45, 45, 87, 101, 98, 75, 105, 116, 70, 111, 114, 109, 66, 111, 117,
            110, 100, 97, 114, 121, 117, 55, 111, 120, 69, 53, 84, 51, 85, 67, 50, 120, 89, 50, 81,
            57, 13, 10, //End of boundary
            //Message-0
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 100, 116, 70, 114, 111, 109, 34, 13, 10, 13, 10, 50, 13, 10,
            //Start of boundary
            45, 45, 45, 45, 45, 45, 87, 101, 98, 75, 105, 116, 70, 111, 114, 109, 66, 111, 117, 110,
            100, 97, 114, 121, 117, 55, 111, 120, 69, 53, 84, 51, 85, 67, 50, 120, 89, 50, 81, 57,
            13, 10, //End of boundary
            //Message-1
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 100, 116, 70, 114, 111, 109, 79, 112, 116, 34, 13, 10, 13, 10, 51, 13, 10,
            //Start of boundary
            45, 45, 45, 45, 45, 45, 87, 101, 98, 75, 105, 116, 70, 111, 114, 109, 66, 111, 117, 110,
            100, 97, 114, 121, 117, 55, 111, 120, 69, 53, 84, 51, 85, 67, 50, 120, 89, 50, 81, 57,
            13, 10, //Message-2
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 102, 105, 108, 101, 34, 59, 32, 102, 105, 108, 101, 110, 97, 109, 101, 61, 34, 116,
            101, 115, 116, 45, 112, 97, 121, 108, 111, 97, 100, 46, 116, 120, 116, 34, 13, 10, 67,
            111, 110, 116, 101, 110, 116, 45, 84, 121, 112, 101, 58, 32, 116, 101, 120, 116, 47,
            112, 108, 97, 105, 110, 13, 10, 13, 10, 49, 50, 51, 13, 10,
            //Start of boundary
            45, 45, 45, 45, 45, 45, 87, 101, 98, 75, 105, 116, 70, 111, 114, 109, 66, 111, 117, 110,
            100, 97, 114, 121, 117, 55, 111, 120, 69, 53, 84, 51, 85, 67, 50, 120, 89, 50, 81, 57,
            45, 45, 13, 10,
        ];

        let result: Vec<&[u8]> = ContentIterator::new(payload.as_slice(), boundary).collect();

        let expected_payload_0: Vec<u8> = vec![
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 100, 116, 70, 114, 111, 109, 34, 13, 10, 13, 10, 50, 13, 10,
        ];

        let expected_payload_1: Vec<u8> = vec![
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 100, 116, 70, 114, 111, 109, 79, 112, 116, 34, 13, 10, 13, 10, 51, 13, 10,
        ];

        let expected_payload_2: Vec<u8> = vec![
            67, 111, 110, 116, 101, 110, 116, 45, 68, 105, 115, 112, 111, 115, 105, 116, 105, 111,
            110, 58, 32, 102, 111, 114, 109, 45, 100, 97, 116, 97, 59, 32, 110, 97, 109, 101, 61,
            34, 102, 105, 108, 101, 34, 59, 32, 102, 105, 108, 101, 110, 97, 109, 101, 61, 34, 116,
            101, 115, 116, 45, 112, 97, 121, 108, 111, 97, 100, 46, 116, 120, 116, 34, 13, 10, 67,
            111, 110, 116, 101, 110, 116, 45, 84, 121, 112, 101, 58, 32, 116, 101, 120, 116, 47,
            112, 108, 97, 105, 110, 13, 10, 13, 10, 49, 50, 51, 13, 10,
        ];

        assert_eq!(result.get(0).unwrap(), &expected_payload_0);
        assert_eq!(result.get(1).unwrap(), &expected_payload_1);
        assert_eq!(result.get(2).unwrap(), &expected_payload_2);
    }

    #[test]
    fn test_from_real_example() {
        let src =  "----dio-boundary-0620928629content-disposition: form-data; name=\"IsLocked\"\r\n\r\ntrue\r\n----dio-boundary-0620928629\r\ncontent-disposition: form-data; name=\"BalanceId\"\r\n\r\nSP-0256e9b5829a4a60b6626d527ef3d795ETH\r\n----dio-boundary-0620928629--\r\n";

        let boundary = "--dio-boundary-0620928629";

        let result: Vec<&[u8]> = ContentIterator::new(src.as_bytes(), boundary).collect();

        assert_eq!(result.len(), 2);

        let item: FormDataItem<'_> = FormDataItem::try_parse(result.get(0).unwrap()).unwrap();

        assert_eq!(item.get_name(), "IsLocked");
        assert_eq!(item.unwrap_as_string().unwrap(), "true");

        let item: FormDataItem<'_> = FormDataItem::try_parse(result.get(1).unwrap()).unwrap();
        assert_eq!(item.get_name(), "BalanceId");
        assert_eq!(
            item.unwrap_as_string().unwrap(),
            "SP-0256e9b5829a4a60b6626d527ef3d795ETH"
        );
    }
}
