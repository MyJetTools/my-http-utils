pub struct UrlEncodedBody {
    pub data: String,
}

impl Default for UrlEncodedBody {
    fn default() -> Self {
        Self::new()
    }
}

impl UrlEncodedBody {
    pub fn new() -> Self {
        UrlEncodedBody {
            data: String::new(),
        }
    }
    pub fn append(mut self, key: &str, value: &str) -> Self {
        if !self.data.is_empty() {
            self.data.push('&');
        }

        crate::encode_to_url_string_and_copy(&mut self.data, key);
        self.data.push('=');
        crate::encode_to_url_string_and_copy(&mut self.data, value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::UrlEncodedBody;

    #[test]
    fn test_append_encodes() {
        let body = UrlEncodedBody::new()
            .append("user", "john doe")
            .append("sym", "a&b=c");
        assert_eq!(body.data, "user=john+doe&sym=a%26b%3Dc");
    }

    #[test]
    fn test_non_ascii_is_percent_encoded() {
        // Regression: non-ASCII used to be emitted raw (and '\r' produced "#0D").
        let body = UrlEncodedBody::new().append("k", "Мир");
        assert_eq!(body.data, "k=%D0%9C%D0%B8%D1%80");
    }
}
