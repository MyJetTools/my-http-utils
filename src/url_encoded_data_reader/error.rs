use crate::url_decoder::UrlDecodeError;

#[derive(Debug)]
pub enum ReadingEncodedDataError {
    RequiredParameterIsMissing(String),
    CanNotParseValue(String),
    UrlDecodeError(UrlDecodeError),
}

impl From<UrlDecodeError> for ReadingEncodedDataError {
    fn from(src: UrlDecodeError) -> Self {
        Self::UrlDecodeError(src)
    }
}
