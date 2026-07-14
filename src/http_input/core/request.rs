use crate::url_encoded_data_reader::UrlEncodedValue;

use crate::http_input::{HttpInputValue, HttpParseError};

use super::body_content::{BodyContentType, HttpRequestBodyContent};
use super::data_src::{SRC_HEADER, SRC_PATH};

/// The single abstraction the server-independent `parse` reads through: a transport-free view of
/// an incoming request. The server implements it over its concrete request (hyper headers, a
/// route-matched path, an already-received body); tests implement it over in-memory data.
///
/// `my-http-utils` owns all the parsing on top of these primitives — query-string decoding,
/// header/path lookup, body content-type dispatch (JSON / url-encoded / multipart), and the
/// value→field conversions. An implementor only has to surface the raw pieces below.
///
/// The body is expected to be already received (`parse` is synchronous). Use the generated
/// `READS_BODY` const to decide whether the body needs to be read before calling `parse`.
pub trait THttpRequest {
    /// The raw query string — everything after `?`, without the leading `?`. `""` when absent.
    fn get_query_string(&self) -> &str;

    /// A header value by name, matched **case-insensitively**. Not percent-decoded.
    fn get_header(&self, name: &str) -> Option<&str>;

    /// The raw (still percent-encoded) value of the named path segment. The implementor has
    /// already matched the route template, so this is a by-name lookup.
    fn get_path_value(&self, name: &str) -> Option<&str>;

    /// The full request body. Empty slice when there is no body.
    fn get_body(&self) -> &[u8];

    /// The `Content-Type`, used to choose the body parser. Defaults to the `content-type`
    /// header, so implementors rarely override it.
    fn get_content_type(&self) -> Option<&str> {
        self.get_header("content-type")
    }
}

/// Reads a required path value (path fields are never `Option`). The raw segment is wrapped as
/// a url-encoded value, so it is percent-decoded on conversion — matching the server.
pub fn read_path_value<'s, R: THttpRequest + ?Sized>(
    request: &'s R,
    name: &'static str,
) -> Result<HttpInputValue<'s>, HttpParseError> {
    match request.get_path_value(name) {
        Some(raw) => Ok(HttpInputValue::from_url_encoded(
            UrlEncodedValue::new(name.to_string(), raw),
            SRC_PATH,
        )),
        None => Err(HttpParseError::required(name, SRC_PATH)),
    }
}

pub fn read_header_optional<'s, R: THttpRequest + ?Sized>(
    request: &'s R,
    name: &'static str,
) -> Option<HttpInputValue<'s>> {
    request
        .get_header(name)
        .map(|value| HttpInputValue::from_header(name, value))
}

pub fn read_header_required<'s, R: THttpRequest + ?Sized>(
    request: &'s R,
    name: &'static str,
) -> Result<HttpInputValue<'s>, HttpParseError> {
    read_header_optional(request, name).ok_or_else(|| HttpParseError::required(name, SRC_HEADER))
}

/// The whole request body as [`HttpRequestBodyContent`], for a non-Option `#[http_body_raw]`
/// field. Unlike [`super::BodyReader`] this does **not** parse the body per content-type — the raw
/// body is handed straight to the field's `TryInto` (bytes / String / a JSON deserialize of the
/// whole body), so an array/scalar/binary/malformed body is never rejected up front.
pub fn read_raw_body<R: THttpRequest + ?Sized>(request: &R) -> HttpRequestBodyContent {
    let content_type = request
        .get_content_type()
        .and_then(|c| BodyContentType::from_content_type(c).ok())
        .unwrap_or(BodyContentType::Unknown);

    HttpRequestBodyContent::new(request.get_body().to_vec(), content_type)
}
