use crate::{url_decoder::UrlDecodeError, WebContentType};
use hyper::{Body, Response};

#[derive(Debug)]
pub struct HttpFailResult {
    pub content_type: WebContentType,
    pub status_code: u16,
    pub content: Vec<u8>,
    pub metric_it: bool,
}

impl HttpFailResult {
    pub fn as_query_parameter_required(param_name: &str) -> Self {
        Self {
            content_type: WebContentType::Text,
            content: format!("Query parameter '{}' is required", param_name).into_bytes(),
            status_code: 400,
            metric_it: true,
        }
    }

    pub fn as_header_parameter_required(param_name: &str) -> Self {
        Self {
            content_type: WebContentType::Text,
            content: format!("Query parameter '{}' is required", param_name).into_bytes(),
            status_code: 400,
            metric_it: true,
        }
    }

    pub fn as_path_parameter_required(param_name: &str) -> Self {
        Self {
            content_type: WebContentType::Text,
            content: format!("Path parameter '{}' is required", param_name).into_bytes(),
            status_code: 400,
            metric_it: true,
        }
    }

    pub fn as_not_found(text: String, metric_it: bool) -> Self {
        Self {
            content_type: WebContentType::Text,
            content: text.into_bytes(),
            status_code: 404,
            metric_it,
        }
    }

    pub fn as_unauthorized(text: Option<String>) -> Self {
        Self {
            content_type: WebContentType::Text,
            content: if let Some(text) = text {
                format!("Unauthorized request: {}", text).into_bytes()
            } else {
                format!("Unauthorized request").into_bytes()
            },
            status_code: 401,
            metric_it: true,
        }
    }

    pub fn as_forbidden(text: Option<String>) -> Self {
        Self {
            content_type: WebContentType::Text,
            content: if let Some(text) = text {
                text.into_bytes()
            } else {
                format!("Forbidden").into_bytes()
            },
            status_code: 403,
            metric_it: true,
        }
    }
}

impl Into<Response<Body>> for HttpFailResult {
    fn into(self) -> Response<Body> {
        Response::builder()
            .header("Content-Type", self.content_type.to_string())
            .status(self.status_code)
            .body(Body::from(self.content))
            .unwrap()
    }
}

impl From<UrlDecodeError> for HttpFailResult {
    fn from(src: UrlDecodeError) -> Self {
        Self {
            status_code: 501,
            content_type: WebContentType::Text,
            content: format!("UrlDecodeError: {}", src.msg).into_bytes(),
            metric_it: true,
        }
    }
}
