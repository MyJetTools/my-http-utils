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
            Self::Empty => None,
        }
    }

    pub fn into_vec(self) -> Vec<u8> {
        match self {
            Self::Json(data) => data,
            Self::UrlEncoded(body) => body.data.into_bytes(),
            Self::FormData(body) => body.into_bytes(),
            Self::Raw { data, .. } => data,
            Self::Empty => Vec::new(),
        }
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
