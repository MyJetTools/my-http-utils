use std::str::Utf8Error;

#[derive(Debug)]
pub struct UrlDecodeError {
    pub msg: String,
}

impl From<Utf8Error> for UrlDecodeError {
    fn from(src: Utf8Error) -> Self {
        return Self {
            msg: format!("Can not decode Utf8 string. Reason: {}", src),
        };
    }
}
