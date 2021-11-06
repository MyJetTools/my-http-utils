#[derive(Debug, Clone)]
pub enum WebContentType {
    Html,
    Css,
    JavaScript,
    Json,
    Text,
}

impl WebContentType {
    pub fn to_string(&self) -> &str {
        match self {
            WebContentType::Html => "text/html",
            WebContentType::Css => "text/css",
            WebContentType::JavaScript => "text/javascript",
            WebContentType::Json => "application/json",
            WebContentType::Text => "text/plain; charset=utf-8",
        }
    }
}
