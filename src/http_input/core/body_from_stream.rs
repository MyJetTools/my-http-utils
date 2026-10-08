//! Reading a request body that comes as a stream — a [`rust_extensions::AsyncBytesStream`] — for
//! the derive-generated `parse_with_body_stream`. The body is never asked to be in memory first:
//! the stream is handed in, and only what the model reads out of it is kept.

use my_json::j_path::{JPathReader, JPropName};
use my_json::json_reader::{JsonFirstLineIteratorAsync, JsonParseError, JsonStreamError};
use rust_extensions::AsyncBytesStream;

use crate::http_input::HttpParseError;

use super::{BodyContentType, BodyReader};

/// What a model reads by name out of a body that comes as a stream — received, and then read
/// field by field through the very [`BodyReader`] `parse` reads a whole body with, so a field
/// reads the same value either way.
///
/// * **A JSON body is never put together whole.** It is read member by member as its bytes
///   arrive (my-json's `JsonFirstLineIteratorAsync`), and only the members the model names are
///   kept — verbatim: the key as written, the value byte for byte. Everything else passes through
///   the reader's buffer and is let go. Once every member the model names is read, the stream is
///   not asked for more.
/// * **Any other body** — `x-www-form-urlencoded`, `multipart/form-data`, or one with no
///   `Content-Type` to tell what it is — is read to its end and parsed exactly as a whole body is:
///   those formats can only be read whole.
///
/// A JSON body is read eagerly, so a malformed one fails as [`HttpParseError::InvalidBodyFormat`]
/// as soon as the broken part arrives — the lazy whole-body reader instead reports a field it
/// could not find in it as missing.
pub struct BodyFromStream<'s> {
    content: Vec<u8>,
    content_type: Option<&'s str>,
}

impl<'s> BodyFromStream<'s> {
    /// `names` are the names the model's body fields are read by — what
    /// [`BodyReader::get_optional`] is going to be asked for.
    pub async fn read<TStream, TError>(
        stream: TStream,
        content_type: Option<&'s str>,
        names: &[&str],
    ) -> Result<Self, HttpParseError>
    where
        TStream: AsyncBytesStream<TError>,
        TError: Into<HttpParseError>,
    {
        // The same reading of the header as the whole-body reader's, so a multipart body without
        // a boundary fails the same way — and before anything is read
        let is_json = match content_type {
            Some(content_type) => {
                BodyContentType::from_content_type(content_type)? == BodyContentType::Json
            }
            None => false,
        };

        let content = if is_json {
            read_json_members(stream, names).await?
        } else {
            read_raw_body_from_stream(stream).await?
        };

        Ok(Self {
            content,
            content_type,
        })
    }

    /// The reader the body fields are read through — over the members kept of a JSON body, or
    /// over the whole of any other one.
    pub fn get_body_reader(&self) -> Result<BodyReader<'_>, HttpParseError> {
        BodyReader::from_parts(&self.content, self.content_type)
    }
}

/// The whole body out of a stream — for a non-`Option` `#[http_body_raw]` field, which takes the
/// body as it is (see [`super::read_raw_body`]), and for a body that can only be parsed whole.
///
/// The `into_vec()` the trait comes with is not used: it allocates the size the stream announces
/// up front, and for an incoming body that is whatever the client wrote into `Content-Length`.
pub async fn read_raw_body_from_stream<TStream, TError>(
    stream: TStream,
) -> Result<Vec<u8>, HttpParseError>
where
    TStream: AsyncBytesStream<TError>,
    TError: Into<HttpParseError>,
{
    let mut result = Vec::new();

    while let Some(chunk) = stream.get_next().await.map_err(Into::into)? {
        result.extend_from_slice(&chunk);
    }

    Ok(result)
}

/// Reads the members `names` are looked up in out of a JSON object that comes as a stream, and
/// puts them together into an object of their own — `{"amount":12.5,"note":"hi"}`. A member is
/// kept exactly as it is in the body, so looking a name up in the result finds what it would find
/// in the whole body.
async fn read_json_members<TStream, TError>(
    stream: TStream,
    names: &[&str],
) -> Result<Vec<u8>, HttpParseError>
where
    TStream: AsyncBytesStream<TError>,
    TError: Into<HttpParseError>,
{
    let mut wanted: Vec<&str> = names.iter().map(|name| top_level_key(name)).collect();
    wanted.sort_unstable();
    wanted.dedup();

    let mut result = vec![b'{'];
    let mut members = JsonFirstLineIteratorAsync::new(stream);

    // Once all of them are read, the rest of the body is not read at all
    while !wanted.is_empty() {
        let Some(member) = members.get_next().await else {
            // The `}` of the object
            break;
        };

        let (key, value) = member.map_err(from_json_stream_error)?;

        // Escapes resolved — the key `my_json::j_path::get_value` compares a name with
        let key_str = key.as_str().map_err(from_json_error)?;

        // `get_value` finds the first member with the key, so only the first one is kept
        if let Some(index) = wanted.iter().position(|name| *name == key_str.as_str()) {
            wanted.swap_remove(index);

            if result.len() > 1 {
                result.push(b',');
            }

            result.extend_from_slice(key.as_slice());
            result.push(b':');
            result.extend_from_slice(value.as_slice());
        }
    }

    result.push(b'}');

    Ok(result)
}

/// The key of the member a name is looked up in. A name is a path to
/// `my_json::j_path::get_value` (`user.name`, `items[0]`), and what the path finds is inside the
/// member its first segment names — so that member is kept whole.
fn top_level_key(name: &str) -> &str {
    let len = match JPathReader::new(name).get_prop_name() {
        JPropName::Name(key) | JPropName::Array(key) => key.len(),
        JPropName::ArrayAndIndex { j_prop_name, .. } => j_prop_name.len(),
    };

    // The key is where the path begins
    &name[..len]
}

fn from_json_stream_error<TError: Into<HttpParseError>>(
    err: JsonStreamError<TError>,
) -> HttpParseError {
    match err {
        JsonStreamError::Stream(err) => err.into(),
        JsonStreamError::Json(err) => from_json_error(err),
    }
}

fn from_json_error(err: JsonParseError) -> HttpParseError {
    match err {
        // What the whole-body reader says of a body which is not an object, for the same body
        JsonParseError::CanNotFindStartOfTheJsonObject(_) => HttpParseError::InvalidBodyFormat(
            "JSON body must be an object to read named fields from it".to_string(),
        ),
        err => HttpParseError::InvalidBodyFormat(format!(
            "JSON body is malformed: {}",
            err.to_string()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::top_level_key;

    #[test]
    fn a_name_is_looked_up_in_the_member_its_path_begins_with() {
        assert_eq!(top_level_key("amount"), "amount");
        assert_eq!(top_level_key("user.name"), "user");
        assert_eq!(top_level_key("items[0]"), "items");
        assert_eq!(top_level_key("items[0].id"), "items");
        assert_eq!(top_level_key("items[]"), "items");
        // Not an index - j_path takes it for a key as it is
        assert_eq!(top_level_key("weird]"), "weird]");
        assert_eq!(top_level_key(""), "");
    }
}
