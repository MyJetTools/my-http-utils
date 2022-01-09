use crate::{HttpFailResult, WebContentType};
use hyper::{Body, Response, StatusCode};
use serde::Serialize;

#[derive(Clone)]
pub enum HttpOkResult {
    Ok,

    Content {
        content_type: Option<WebContentType>,
        content: Vec<u8>,
    },
    Text {
        text: String,
    },

    Redirect {
        url: String,
    },
}

impl HttpOkResult {
    pub fn create_json_response<T: Serialize>(model: T) -> HttpOkResult {
        let json = serde_json::to_vec(&model).unwrap();
        HttpOkResult::Content {
            content_type: Some(WebContentType::Json),
            content: json,
        }
    }

    pub fn create_as_usize(number: usize) -> HttpOkResult {
        HttpOkResult::Content {
            content_type: Some(WebContentType::Text),
            content: number.to_string().into_bytes(),
        }
    }

    pub fn redirect(src: &str) -> HttpOkResult {
        HttpOkResult::Redirect {
            url: src.to_string(),
        }
    }
}

impl Into<HttpOkResult> for String {
    fn into(self) -> HttpOkResult {
        HttpOkResult::Content {
            content_type: Some(WebContentType::Text),
            content: self.into_bytes(),
        }
    }
}

impl Into<Result<HttpOkResult, HttpFailResult>> for HttpOkResult {
    fn into(self) -> Result<HttpOkResult, HttpFailResult> {
        Ok(self)
    }
}

impl Into<Response<Body>> for HttpOkResult {
    fn into(self) -> Response<Body> {
        return match self {
            HttpOkResult::Ok => Response::builder()
                .header("Content-Type", WebContentType::Text.to_string())
                .status(200)
                .body(Body::from("OK"))
                .unwrap(),
            HttpOkResult::Content {
                content_type,
                content,
            } => match content_type {
                Some(content_type) => Response::builder()
                    .header("Content-Type", content_type.to_string())
                    .status(200)
                    .body(Body::from(content))
                    .unwrap(),
                None => Response::builder()
                    .status(200)
                    .body(Body::from(content))
                    .unwrap(),
            },
            HttpOkResult::Text { text } => Response::builder()
                .header("Content-Type", WebContentType::Text.to_string())
                .status(200)
                .body(Body::from(text))
                .unwrap(),
            HttpOkResult::Redirect { url } => Response::builder()
                .status(StatusCode::PERMANENT_REDIRECT)
                .header("Location", url)
                .body(Body::empty())
                .unwrap(),
        };
    }
}
