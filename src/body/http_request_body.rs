use rust_extensions::StrOrString;

use crate::body::{FormDataBody, UrlEncodedBody};

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
        }
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
    pub fn into_vec(self) -> Vec<u8> {
        match self {
            Self::Json(data) => data,
            Self::UrlEncoded(body) => body.data.into_bytes(),
            Self::FormData(body) => body.into_bytes(),
            Self::Raw { data, .. } => data,
            // Not materialisable — the bytes live in the channel, not here.
            Self::Stream(_) => Vec::new(),
            Self::Empty => Vec::new(),
        }
    }
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
