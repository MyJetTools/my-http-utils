use crate::url_encoded_data_reader::UrlEncodedValue;

use super::data_src::{SRC_HEADER, SRC_PATH};
use super::{HttpInputValue, HttpParseError};

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
