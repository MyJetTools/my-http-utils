use rust_extensions::StrOrString;

#[cfg(feature = "compression")]
use crate::body::CompressionLevel;
use crate::body::{FormDataBody, UrlEncodedBody};
#[cfg(feature = "encryption")]
use crate::encryption::AES128GCM_CONTENT_ENCODING;
#[cfg(feature = "encryption")]
use crate::schema::client::HttpRequestBuildError;

/// The `Content-Encoding` token of a gzipped body.
#[cfg(feature = "compression")]
const GZIP_CONTENT_ENCODING: &str = "gzip";

#[cfg(any(feature = "compression", feature = "encryption"))]
const CONTENT_ENCODING_HEADER: &str = "Content-Encoding";

pub enum HttpRequestBody {
    Json(Vec<u8>),
    UrlEncoded(UrlEncodedBody),
    FormData(FormDataBody),
    Raw {
        data: Vec<u8>,
        content_type: Option<&'static str>,
    },
    /// The body is handed over as a **stream** (`#[http_body_as_stream]`) — it never exists in
    /// memory whole. The transport takes the reader out with
    /// [`HttpBodyAsStream::get_body_reader`](crate::http_input::HttpBodyAsStream::get_body_reader)
    /// and writes the chunks as they arrive.
    Stream(crate::http_input::HttpBodyAsStream),
    Empty,
    /// A body that went through `compress` or `encrypt` (the `compression` and `encryption`
    /// features): the bytes as they go on the wire.
    ///
    /// * `data` is what is sent, as it is. It is never empty: an empty body stays [`Self::Empty`].
    /// * `content_type` is the type of the body as it was made. The receiver reads the body as
    ///   that once it has undone the encodings.
    /// * `headers` are what the transport adds to the request for it: `Content-Encoding` with the
    ///   encodings in the order they were applied (`gzip`, `aes128gcm`, `gzip, aes128gcm`), or
    ///   nothing for a body too short to be worth gzipping. See [`Self::get_headers`].
    ///
    /// A transport does not compress it again: an encrypted body does not compress, and a
    /// compressed one is compressed already.
    ///
    /// The variant is there only with the `compression` or the `encryption` feature. A `match`
    /// that names it has to be under the same `cfg`; one that does not should keep a wildcard
    /// arm, since another crate in the build can switch a feature on.
    #[cfg(any(feature = "compression", feature = "encryption"))]
    Encoded {
        data: Vec<u8>,
        content_type: Option<StrOrString<'static>>,
        headers: Vec<(&'static str, String)>,
    },
}

impl HttpRequestBody {
    pub fn empty() -> Self {
        Self::Empty
    }

    pub fn from_raw_data(data: Vec<u8>, content_type: Option<&'static str>) -> Self {
        Self::Raw { data, content_type }
    }

    /// Panics if the value fails to serialize; use [`Self::try_as_json`] to handle the error.
    pub fn as_json(value: &impl serde::Serialize) -> Self {
        Self::try_as_json(value).expect("Failed to serialize to JSON")
    }

    pub fn try_as_json(value: &impl serde::Serialize) -> Result<Self, serde_json::Error> {
        let payload = serde_json::to_vec(value)?;
        Ok(Self::Json(payload))
    }

    pub fn get_content_type(&self) -> Option<StrOrString<'static>> {
        match self {
            Self::Json(_) => Some("application/json".into()),
            Self::UrlEncoded(_) => Some("application/x-www-form-urlencoded".into()),
            Self::FormData(body) => Some(body.get_content_type().into()),
            Self::Raw { content_type, .. } => {
                let content_type = (*content_type)?;
                Some(content_type.into())
            }
            // A stream carries no type of its own: the model states it with an `#[http_header]`
            // field, or the caller adds it on the transport (`with_header`).
            Self::Stream(_) => None,
            Self::Empty => None,
            #[cfg(any(feature = "compression", feature = "encryption"))]
            Self::Encoded { content_type, .. } => content_type.clone(),
        }
    }

    /// The headers a transport adds to the request for this body, besides `Content-Type`: those
    /// of an `Encoded` body, none for the others. Read them, like the content type, before
    /// [`Self::into_vec`] consumes the body.
    ///
    /// `Content-Encoding` is a list: a transport that has one of its own already puts these after
    /// it on the same line (`br, gzip, aes128gcm`).
    ///
    /// There in every build, empty without the `compression` and `encryption` features, so a
    /// transport calls it with no `cfg` of its own.
    pub fn get_headers(&self) -> &[(&'static str, String)] {
        #[cfg(any(feature = "compression", feature = "encryption"))]
        if let Self::Encoded { headers, .. } = self {
            return headers;
        }

        &[]
    }

    /// [`Self::compress_with_level`] at [`CompressionLevel::Default`].
    #[cfg(feature = "compression")]
    pub fn compress(self) -> Self {
        self.compress_with_level(CompressionLevel::Default)
    }

    /// The body gzipped at `level`, as [`Self::Encoded`] with `Content-Encoding: gzip`. The content
    /// type stays the one this body has.
    ///
    /// Some bodies come back as they are, since there is nothing to gain:
    /// * an empty body, and a [`Self::Stream`]: it is never in memory whole, so it goes out as it
    ///   comes;
    /// * an [`Self::Encoded`] body: compressed already, or encrypted, and encrypted data does not
    ///   compress. Compress first, then encrypt.
    ///
    /// A body under 64 bytes only gets bigger gzipped: it comes back as [`Self::Encoded`] with its
    /// bytes as they were and no header.
    #[cfg(feature = "compression")]
    pub fn compress_with_level(self, level: CompressionLevel) -> Self {
        use super::compression::{gzip, MIN_LEN_TO_COMPRESS};

        match self {
            Self::Stream(_) | Self::Empty | Self::Encoded { .. } => self,
            plain => {
                let content_type = plain.get_content_type();
                let data = plain.into_vec();

                if data.is_empty() {
                    return Self::Empty;
                }

                let mut headers = Vec::new();

                let data = if data.len() < MIN_LEN_TO_COMPRESS {
                    data
                } else {
                    add_content_encoding(&mut headers, GZIP_CONTENT_ENCODING);
                    gzip(&data, level)
                };

                Self::Encoded {
                    data,
                    content_type,
                    headers,
                }
            }
        }
    }

    /// The body encrypted with `key`, as [`Self::Encoded`] with `aes128gcm` at the end of its
    /// `Content-Encoding`. The plain text is an `EncryptedBody`: a fresh salt from the OS random
    /// source, the time now, then the bytes of this body, gzipped already if `compress` came
    /// first. The content type stays the one this body has.
    ///
    /// * An empty body stays [`Self::Empty`]: there is nothing to encrypt, and it goes out without
    ///   `Content-Encoding`.
    /// * A [`Self::Stream`] is not supported yet: a stream is never in memory whole.
    /// * A body that is encrypted already is not encrypted twice.
    ///
    /// Both refusals are a [`HttpRequestBuildError`] with `field` `"body"`: the error the rest of
    /// building a request gives, so `encrypt` chains after `get_body` with `?`.
    ///
    /// # Panics
    ///
    /// If the OS random source gives no salt.
    #[cfg(feature = "encryption")]
    pub fn encrypt(
        self,
        key: &crate::encryption::HttpBodyEncryptionKey,
    ) -> Result<Self, HttpRequestBuildError> {
        use crate::encryption::EncryptedBody;

        let (content_type, mut headers, body) = match self {
            Self::Stream(_) => return Err(not_supported("a streamed body can not be encrypted")),
            Self::Encoded { ref headers, .. } if is_encrypted(headers) => {
                return Err(not_supported("the body is encrypted already"));
            }
            Self::Encoded {
                data,
                content_type,
                headers,
            } => (content_type, headers, data),
            plain => (plain.get_content_type(), Vec::new(), plain.into_vec()),
        };

        if body.is_empty() {
            return Ok(Self::Empty);
        }

        let data = EncryptedBody {
            salt: EncryptedBody::random_salt(),
            timestamp: rust_extensions::date_time::DateTimeAsMicroseconds::now(),
            body,
        }
        .encrypt(key);

        add_content_encoding(&mut headers, AES128GCM_CONTENT_ENCODING);

        Ok(Self::Encoded {
            data,
            content_type,
            headers,
        })
    }

    /// `true` for [`Self::Stream`] — the one variant that has no bytes to give.
    ///
    /// A transport that materialises bodies must check this (or match `Stream` explicitly) before
    /// calling [`Self::into_vec`]; see the warning there.
    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream(_))
    }

    /// Materialises the body into bytes.
    ///
    /// **A transport MUST match [`Self::Stream`] before it gets here.** A streamed body has no
    /// bytes yet — they only exist once the reader is drained — so this returns an **empty**
    /// `Vec`, and a transport that ignores the variant silently sends an empty body instead of the
    /// payload. Use [`Self::is_stream`] to guard the call.
    ///
    /// An `Encoded` body gives its bytes as they go on the wire.
    pub fn into_vec(self) -> Vec<u8> {
        match self {
            Self::Json(data) => data,
            Self::UrlEncoded(body) => body.data.into_bytes(),
            Self::FormData(body) => body.into_bytes(),
            Self::Raw { data, .. } => data,
            // Not materialisable — the bytes live in the channel, not here.
            Self::Stream(_) => Vec::new(),
            Self::Empty => Vec::new(),
            #[cfg(any(feature = "compression", feature = "encryption"))]
            Self::Encoded { data, .. } => data,
        }
    }
}

/// Puts `encoding` at the end of the `Content-Encoding` of `headers`: the encodings are listed in
/// the order they were applied.
#[cfg(any(feature = "compression", feature = "encryption"))]
fn add_content_encoding(headers: &mut Vec<(&'static str, String)>, encoding: &str) {
    match headers
        .iter_mut()
        .find(|(name, _)| *name == CONTENT_ENCODING_HEADER)
    {
        Some((_, value)) => {
            value.push_str(", ");
            value.push_str(encoding);
        }
        None => headers.push((CONTENT_ENCODING_HEADER, encoding.to_string())),
    }
}

#[cfg(feature = "encryption")]
fn is_encrypted(headers: &[(&'static str, String)]) -> bool {
    headers.iter().any(|(name, value)| {
        *name == CONTENT_ENCODING_HEADER
            && value
                .split(',')
                .any(|encoding| encoding.trim() == AES128GCM_CONTENT_ENCODING)
    })
}

#[cfg(feature = "encryption")]
fn not_supported(what: &str) -> HttpRequestBuildError {
    HttpRequestBuildError::new("body", format!("not supported: {what}"))
}

impl From<crate::http_input::HttpBodyAsStream> for HttpRequestBody {
    fn from(src: crate::http_input::HttpBodyAsStream) -> Self {
        HttpRequestBody::Stream(src)
    }
}

impl From<UrlEncodedBody> for HttpRequestBody {
    fn from(src: UrlEncodedBody) -> Self {
        HttpRequestBody::UrlEncoded(src)
    }
}

impl From<FormDataBody> for HttpRequestBody {
    fn from(src: FormDataBody) -> Self {
        HttpRequestBody::FormData(src)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::http_input::HttpBodyAsStream;

    fn content_type_of(body: &HttpRequestBody) -> Option<String> {
        body.get_content_type()
            .map(|content_type| content_type.as_str().to_string())
    }

    fn headers_of(body: &HttpRequestBody) -> Vec<(&'static str, String)> {
        body.get_headers().to_vec()
    }

    fn content_encoding(value: &str) -> [(&'static str, String); 1] {
        [("Content-Encoding", value.to_string())]
    }

    #[cfg(any(feature = "compression", feature = "encryption"))]
    #[test]
    fn an_encoded_body_goes_with_its_content_type_and_its_headers() {
        let body = HttpRequestBody::Encoded {
            data: vec![1, 2, 3],
            content_type: Some("application/json".into()),
            headers: content_encoding("gzip, aes128gcm").to_vec(),
        };

        assert_eq!(content_type_of(&body).as_deref(), Some("application/json"));
        assert_eq!(headers_of(&body), content_encoding("gzip, aes128gcm"));
        assert!(!body.is_stream());
        assert_eq!(body.into_vec(), vec![1, 2, 3]);
    }

    #[test]
    fn a_body_as_it_was_made_adds_no_headers() {
        for body in [
            HttpRequestBody::Empty,
            HttpRequestBody::Json(b"{}".to_vec()),
            HttpRequestBody::from_raw_data(b"x".to_vec(), Some("text/plain")),
            HttpRequestBody::UrlEncoded(UrlEncodedBody::new().append("a", "1")),
            HttpRequestBody::Stream(HttpBodyAsStream::empty()),
        ] {
            assert!(body.get_headers().is_empty());
        }
    }

    #[cfg(feature = "compression")]
    mod compress {
        use super::*;

        use crate::body::compression::tests::{compressible_body, gunzip, LEVELS};

        #[test]
        fn a_body_is_gzipped_and_keeps_its_content_type() {
            let body = HttpRequestBody::Json(compressible_body()).compress();

            assert_eq!(content_type_of(&body).as_deref(), Some("application/json"));
            assert_eq!(headers_of(&body), content_encoding("gzip"));

            let data = body.into_vec();
            assert!(data.len() < compressible_body().len());
            assert_eq!(gunzip(&data), compressible_body());
        }

        #[test]
        fn every_level_gzips() {
            for level in LEVELS {
                let body = HttpRequestBody::from_raw_data(compressible_body(), Some("text/plain"))
                    .compress_with_level(level);

                assert_eq!(headers_of(&body), content_encoding("gzip"), "{level:?}");
                assert_eq!(gunzip(&body.into_vec()), compressible_body(), "{level:?}");
            }
        }

        #[test]
        fn a_body_under_64_bytes_goes_as_it_is() {
            let body = HttpRequestBody::Json(br#"{"a":1}"#.to_vec()).compress();

            assert_eq!(content_type_of(&body).as_deref(), Some("application/json"));
            assert!(body.get_headers().is_empty());
            assert_eq!(body.into_vec(), br#"{"a":1}"#);
        }

        #[test]
        fn an_empty_body_and_a_stream_stay_as_they_are() {
            assert!(matches!(
                HttpRequestBody::Empty.compress(),
                HttpRequestBody::Empty
            ));
            assert!(matches!(
                HttpRequestBody::Json(Vec::new()).compress(),
                HttpRequestBody::Empty
            ));
            assert!(HttpRequestBody::Stream(HttpBodyAsStream::empty())
                .compress()
                .is_stream());
        }

        #[test]
        fn a_compressed_body_is_not_compressed_again() {
            let twice = HttpRequestBody::Json(compressible_body())
                .compress()
                .compress();

            assert_eq!(headers_of(&twice), content_encoding("gzip"));
            assert_eq!(gunzip(&twice.into_vec()), compressible_body());
        }
    }

    #[cfg(feature = "encryption")]
    mod encrypt {
        use super::*;

        use crate::encryption::{EncryptedBody, HttpBodyEncryptionKey};
        use rust_extensions::date_time::DateTimeAsMicroseconds;

        fn key() -> HttpBodyEncryptionKey {
            HttpBodyEncryptionKey::from_bytes(&[1; 32]).unwrap()
        }

        /// The content type the body goes out with, and what the receiver decrypts.
        fn open(body: HttpRequestBody) -> (Option<String>, EncryptedBody) {
            let content_type = content_type_of(&body);
            assert_eq!(headers_of(&body), content_encoding("aes128gcm"));

            let HttpRequestBody::Encoded { data, .. } = body else {
                panic!("expected an encoded body");
            };

            (content_type, EncryptedBody::decrypt(&data, &key()).unwrap())
        }

        fn salt_of(body: HttpRequestBody) -> Vec<u8> {
            body.into_vec()[..16].to_vec()
        }

        #[test]
        fn a_json_body_is_encrypted_with_the_time_and_keeps_its_content_type() {
            let before = DateTimeAsMicroseconds::now().unix_microseconds;
            let body = HttpRequestBody::Json(br#"{"a":1}"#.to_vec())
                .encrypt(&key())
                .unwrap();
            let after = DateTimeAsMicroseconds::now().unix_microseconds;

            let (content_type, opened) = open(body);

            assert_eq!(content_type.as_deref(), Some("application/json"));
            assert_eq!(opened.body, br#"{"a":1}"#);
            assert!((before..=after).contains(&opened.timestamp.unix_microseconds));
        }

        #[test]
        fn every_body_keeps_its_content_type() {
            // Built twice: once to encrypt, once for the bytes the receiver must get back. The
            // boundary of the form comes from the string given, so both are the same.
            let url_encoded = || UrlEncodedBody::new().append("a", "1");
            let form_data =
                || FormDataBody::new("TESTBOUNDARY").append_form_data_field("name", "value");

            let cases = [
                (
                    HttpRequestBody::from_raw_data(b"hosts: {}\n".to_vec(), Some("text/yaml")),
                    Some("text/yaml".to_string()),
                    b"hosts: {}\n".to_vec(),
                ),
                (
                    HttpRequestBody::from_raw_data(b"\x00\x01".to_vec(), None),
                    None,
                    b"\x00\x01".to_vec(),
                ),
                (
                    HttpRequestBody::UrlEncoded(url_encoded()),
                    Some("application/x-www-form-urlencoded".to_string()),
                    url_encoded().data.into_bytes(),
                ),
                (
                    HttpRequestBody::FormData(form_data()),
                    Some(form_data().get_content_type()),
                    form_data().into_bytes(),
                ),
            ];

            for (body, content_type, plain) in cases {
                let (sent_type, opened) = open(body.encrypt(&key()).unwrap());

                assert_eq!(sent_type, content_type);
                assert_eq!(opened.body, plain);
            }
        }

        #[test]
        fn every_encryption_takes_a_fresh_salt() {
            let encrypt = || {
                HttpRequestBody::Json(b"{}".to_vec())
                    .encrypt(&key())
                    .unwrap()
            };

            assert_ne!(salt_of(encrypt()), salt_of(encrypt()));
        }

        #[test]
        fn an_empty_body_stays_empty() {
            for body in [
                HttpRequestBody::Empty,
                HttpRequestBody::Json(Vec::new()),
                HttpRequestBody::from_raw_data(Vec::new(), Some("application/json")),
            ] {
                assert!(matches!(body.encrypt(&key()), Ok(HttpRequestBody::Empty)));
            }
        }

        #[test]
        fn a_stream_is_not_supported() {
            let err = HttpRequestBody::Stream(HttpBodyAsStream::empty())
                .encrypt(&key())
                .err()
                .expect("a stream does not encrypt");

            assert_eq!(err.field, "body");
            assert!(err.reason.starts_with("not supported"), "{err}");
        }

        #[test]
        fn an_encrypted_body_is_not_encrypted_twice() {
            let once = HttpRequestBody::Json(b"{}".to_vec())
                .encrypt(&key())
                .unwrap();

            let err = once
                .encrypt(&key())
                .err()
                .expect("a body is not encrypted twice");

            assert_eq!(err.field, "body");
            assert!(err.reason.starts_with("not supported"), "{err}");
        }
    }

    #[cfg(all(feature = "compression", feature = "encryption"))]
    mod compress_then_encrypt {
        use super::*;

        use crate::body::compression::tests::{compressible_body, gunzip};
        use crate::encryption::{EncryptedBody, HttpBodyEncryptionKey};

        fn key() -> HttpBodyEncryptionKey {
            HttpBodyEncryptionKey::from_bytes(&[1; 32]).unwrap()
        }

        fn decrypt(body: HttpRequestBody) -> Vec<u8> {
            EncryptedBody::decrypt(&body.into_vec(), &key())
                .unwrap()
                .body
        }

        #[test]
        fn a_body_is_gzipped_then_encrypted() {
            let body = HttpRequestBody::Json(compressible_body())
                .compress()
                .encrypt(&key())
                .unwrap();

            assert_eq!(content_type_of(&body).as_deref(), Some("application/json"));
            assert_eq!(headers_of(&body), content_encoding("gzip, aes128gcm"));
            assert_eq!(gunzip(&decrypt(body)), compressible_body());
        }

        #[test]
        fn a_body_too_short_to_gzip_is_only_encrypted() {
            let body = HttpRequestBody::Json(br#"{"a":1}"#.to_vec())
                .compress()
                .encrypt(&key())
                .unwrap();

            assert_eq!(headers_of(&body), content_encoding("aes128gcm"));
            assert_eq!(decrypt(body), br#"{"a":1}"#);
        }

        #[test]
        fn an_encrypted_body_is_not_compressed() {
            let body = HttpRequestBody::Json(compressible_body())
                .encrypt(&key())
                .unwrap()
                .compress();

            assert_eq!(headers_of(&body), content_encoding("aes128gcm"));
            assert_eq!(decrypt(body), compressible_body());
        }
    }
}
