use crate::form_data_reader::ReadingFromDataError;
use crate::http_input::core::data_src::SRC_FORM_DATA;
use crate::url_decoder::UrlDecodeError;
use crate::url_encoded_data_reader::ReadingEncodedDataError;

/// Everything the server-independent parse layer can fail with.
///
/// Each variant carries enough structured data for `my-http-server` to reconstruct — without
/// loss — the same `HttpFailResult` (status code + text) it used to produce inline, via a
/// `From<HttpParseError> for HttpFailResult`. That conversion lives on the server; this type
/// stays transport-free and wasm-safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpParseError {
    /// A required parameter was not present. `src` is one of the `data_src::SRC_*` constants.
    RequiredParameterIsMissing { name: String, src: &'static str },
    /// A value was present but could not be parsed into the target type. `value` keeps the
    /// offending textual value so the server can put it into the error message.
    CanNotParseValue {
        name: String,
        src: &'static str,
        value: String,
    },
    /// Percent-decoding of a query-string / path / url-encoded-body value failed.
    UrlDecodeError(String),
    /// The body could not be interpreted as the format its `Content-Type` implied
    /// (e.g. malformed JSON, non-UTF-8 url-encoded body).
    InvalidBodyFormat(String),
    /// The value/body can not be converted to the requested type given its content type
    /// (e.g. a form-data value where a file was expected).
    NotSupportedContentType(String),
    /// The conversion is forbidden for this source (e.g. reading a file out of a query-string or
    /// JSON value) — the reference server answered 403 here, so this maps back to `as_forbidden`.
    Forbidden(String),
    /// A field `validator` rejected the value.
    Validation(String),
}

impl HttpParseError {
    pub fn required(name: impl Into<String>, src: &'static str) -> Self {
        Self::RequiredParameterIsMissing {
            name: name.into(),
            src,
        }
    }

    pub fn cannot_parse(
        name: impl Into<String>,
        src: &'static str,
        value: impl Into<String>,
    ) -> Self {
        Self::CanNotParseValue {
            name: name.into(),
            src,
            value: value.into(),
        }
    }

    /// Maps a low-level [`ReadingEncodedDataError`] (from `UrlEncodedValue`) into the richer
    /// parse error, attaching the caller's `name`/`src` context. Mirrors the server's
    /// `url_encoded_data::convert_error`.
    pub fn from_reading_encoded(name: &str, err: ReadingEncodedDataError, src: &'static str) -> Self {
        match err {
            ReadingEncodedDataError::RequiredParameterIsMissing(missing) => {
                Self::RequiredParameterIsMissing { name: missing, src }
            }
            ReadingEncodedDataError::CanNotParseValue(value) => Self::CanNotParseValue {
                name: name.to_string(),
                src,
                value,
            },
            ReadingEncodedDataError::UrlDecodeError(err) => Self::UrlDecodeError(err.msg),
        }
    }
}

/// Adapter around a `Result<T, ReadingEncodedDataError>` (as returned by `UrlEncodedValue`),
/// carrying the `name`/`src` context needed for a good error message.
pub(crate) fn convert_reading_error<T>(
    name: &str,
    result: Result<T, ReadingEncodedDataError>,
    src: &'static str,
) -> Result<T, HttpParseError> {
    result.map_err(|err| HttpParseError::from_reading_encoded(name, err, src))
}

impl From<UrlDecodeError> for HttpParseError {
    fn from(src: UrlDecodeError) -> Self {
        Self::UrlDecodeError(src.msg)
    }
}

impl From<ReadingFromDataError> for HttpParseError {
    fn from(src: ReadingFromDataError) -> Self {
        match src {
            ReadingFromDataError::ParameterMissing(name) => Self::RequiredParameterIsMissing {
                name,
                src: SRC_FORM_DATA,
            },
            ReadingFromDataError::ValidationError { field, error } => {
                Self::NotSupportedContentType(format!("Field '{}': {}", field, error))
            }
        }
    }
}

/// Bridges std's `Infallible` into `HttpParseError` so any `TryInto` whose `Error = Infallible`
/// unifies through `?` in a context that yields `HttpParseError`. Never actually constructs a value.
impl From<std::convert::Infallible> for HttpParseError {
    fn from(never: std::convert::Infallible) -> Self {
        match never {}
    }
}

/// A `#[http_body_raw]` field of type `String` converts via std's `TryFrom<Vec<u8>> for String`
/// (utf-8 check, `Error = FromUtf8Error`); map that into the same parse error the old `as_str`
/// path produced.
impl From<std::string::FromUtf8Error> for HttpParseError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self::InvalidBodyFormat(format!("Body is not valid UTF-8: {}", err))
    }
}

impl std::fmt::Display for HttpParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiredParameterIsMissing { name, src } => {
                write!(f, "Required parameter '{}' is missing. Source: {}", name, src)
            }
            Self::CanNotParseValue { name, src, value } => write!(
                f,
                "Can not parse '{}' value '{}' from {}",
                name, value, src
            ),
            Self::UrlDecodeError(msg) => write!(f, "Url decode error: {}", msg),
            Self::InvalidBodyFormat(msg) => write!(f, "Invalid body format: {}", msg),
            Self::NotSupportedContentType(msg) => write!(f, "Not supported content type: {}", msg),
            Self::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            Self::Validation(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for HttpParseError {}
