#[derive(Debug, Clone)]
pub enum WebContentType {
    Html,
    Css,
    Png,
    Svg,
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
            WebContentType::Png => "image/png",
            WebContentType::Svg => "image/svg+xml",
        }
    }

    pub fn detect_by_extension(path: &str) -> Self {
        let res = path.split(".");

        let el = res.last();

        match el {
            Some(extension) => match extension {
                "png" => WebContentType::Png,
                "svg" => WebContentType::Svg,
                "css" => WebContentType::Css,
                "js" => WebContentType::JavaScript,
                _ => WebContentType::Text,
            },
            None => WebContentType::Text,
        }
    }
}
