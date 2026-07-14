use std::fmt::Display;

use rust_extensions::StrOrString;

pub struct FormDataBody {
    // files: Vec<MultipartFile>,
    boundary: String,
    buffer: Vec<u8>,
}

impl FormDataBody {
    pub fn new(rnd_string: &str) -> Self {
        let boundary = format!("------DataFormBoundary{}", rnd_string);
        Self {
            boundary,
            buffer: vec![], //files: vec![],
        }
    }

    pub fn append_form_data_field(
        mut self,
        name: impl Into<StrOrString<'static>>,
        value: impl Display,
    ) -> Self {
        use std::io::Write;

        let name = name.into();
        write!(
            &mut self.buffer,
            "--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n",
            self.boundary,
            escape_header_param(name.as_str()),
            value
        )
        .unwrap();

        self
    }

    pub fn append_form_data_file(
        mut self,
        name: impl Into<StrOrString<'static>>,
        file_name: impl Into<StrOrString<'static>>,
        content_type: impl Into<StrOrString<'static>>,
        content: &[u8],
    ) -> Self {
        use std::io::Write;

        let name = name.into();
        let file_name = file_name.into();
        let content_type = content_type.into();
        write!(
            &mut self.buffer,
            "--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
            self.boundary,
            escape_header_param(name.as_str()),
            escape_header_param(file_name.as_str()),
            content_type.as_str()
        )
        .unwrap();

        self.buffer.extend_from_slice(content);
        self.buffer.extend_from_slice("\r\n".as_bytes());
        self
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut result = self.buffer;

        result.extend_from_slice(b"--");
        result.extend_from_slice(self.boundary.as_bytes());
        result.extend_from_slice(b"--");

        result
    }

    pub fn get_content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }
}

/// Escapes a `Content-Disposition` parameter value so a crafted field/file name can not
/// break out of its quotes or inject extra header lines. Quotes and CR/LF are
/// percent-encoded; everything else is preserved.
fn escape_header_param(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => result.push_str("%22"),
            '\r' => result.push_str("%0D"),
            '\n' => result.push_str("%0A"),
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use crate::body::FormDataBody;

    #[test]
    fn test_form_data_body() {
        let form_data =
            FormDataBody::new("1234567890123456").append_form_data_field("test", "my value");

        let result = form_data.into_bytes();
        let text = std::str::from_utf8(result.as_slice()).unwrap();

        assert_eq!(
            text,
            "--------DataFormBoundary1234567890123456\r\n\
             Content-Disposition: form-data; name=\"test\"\r\n\r\n\
             my value\r\n\
             --------DataFormBoundary1234567890123456--"
        );
    }

    #[test]
    fn test_form_data_body_2() {
        let form_data = FormDataBody::new("1234567890123456")
            .append_form_data_field("test", "my value")
            .append_form_data_file("name", "file.txt", "text/plain", "123".as_bytes());

        let result = form_data.into_bytes();
        let text = std::str::from_utf8(result.as_slice()).unwrap();

        assert!(text.contains("Content-Disposition: form-data; name=\"name\"; filename=\"file.txt\""));
        assert!(text.contains("Content-Type: text/plain\r\n\r\n123\r\n"));
        assert!(text.ends_with("--------DataFormBoundary1234567890123456--"));
    }

    #[test]
    fn test_field_name_injection_is_escaped() {
        // A name trying to break out of quotes / inject a header must be neutralised.
        let form_data = FormDataBody::new("b").append_form_data_field(
            "evil\"\r\nX-Injected: 1".to_string(),
            "v",
        );
        let text = String::from_utf8(form_data.into_bytes()).unwrap();

        assert!(!text.contains("X-Injected: 1\r\n"));
        assert!(text.contains("name=\"evil%22%0D%0AX-Injected: 1\""));
    }

    #[test]
    fn test_file_name_injection_is_escaped() {
        let form_data = FormDataBody::new("b").append_form_data_file(
            "f".to_string(),
            "a\"b\r\n.txt".to_string(),
            "text/plain".to_string(),
            b"x",
        );
        let text = String::from_utf8(form_data.into_bytes()).unwrap();
        assert!(text.contains("filename=\"a%22b%0D%0A.txt\""));
    }
}
