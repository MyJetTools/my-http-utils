#[derive(Debug, Clone)]
pub enum HttpParameterInputSource {
    Path,
    Query,
    Header,
    BodyModel,
    BodyRaw,
    FormData,
}

impl HttpParameterInputSource {
    pub fn is_query(&self) -> bool {
        matches!(self, HttpParameterInputSource::Query)
    }

    pub fn is_body_raw(&self) -> bool {
        matches!(self, HttpParameterInputSource::BodyModel)
    }

    pub fn is_body(&self) -> bool {
        matches!(
            self,
            HttpParameterInputSource::BodyModel | HttpParameterInputSource::BodyRaw
        )
    }

    pub fn is_form_data(&self) -> bool {
        matches!(self, HttpParameterInputSource::FormData)
    }

    pub fn is_header(&self) -> bool {
        matches!(self, HttpParameterInputSource::Header)
    }

    pub fn as_str(&self) -> &str {
        match self {
            HttpParameterInputSource::Path => "path",
            HttpParameterInputSource::Query => "query",
            HttpParameterInputSource::Header => "header",
            HttpParameterInputSource::BodyModel => "body",
            HttpParameterInputSource::BodyRaw => "body",
            HttpParameterInputSource::FormData => "form_data",
        }
    }
}
