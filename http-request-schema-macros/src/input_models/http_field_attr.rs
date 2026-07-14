use crate::attributes::*;

#[derive(Clone)]
#[allow(clippy::enum_variant_names)] // variants mirror the `Http*Attribute` types by design
pub enum HttpFieldAttribute<'s> {
    HttpHeader(HttpHeaderAttribute<'s>),
    HttpQuery(HttpQueryAttribute<'s>),
    HttpBody(HttpBodyAttribute<'s>),
    HttpFormData(HttpFormDataAttribute<'s>),
    HttpBodyRaw(HttpBodyRawAttribute<'s>),
    HttpPath(HttpPathAttribute<'s>),
}

impl<'s> HttpFieldAttribute<'s> {
    /// Whether a `default = …` was supplied. Only its presence matters to the schema (it
    /// makes the parameter non-required); the default value itself is a server concern.
    pub fn has_default(&self) -> bool {
        match self {
            Self::HttpHeader(a) => a.default.is_some(),
            Self::HttpQuery(a) => a.default.is_some(),
            Self::HttpBody(a) => a.default.is_some(),
            Self::HttpFormData(a) => a.default.is_some(),
            Self::HttpBodyRaw(a) => a.default.is_some(),
            Self::HttpPath(a) => a.default.is_some(),
        }
    }

    /// The parsed `default = …` value, if any — used by the server-side `parse` codegen.
    #[cfg(feature = "server")]
    pub fn get_default(&'s self) -> Option<super::HttpInputDefaultValue<'s>> {
        let default_attr = match self {
            Self::HttpHeader(a) => a.default.clone(),
            Self::HttpQuery(a) => a.default.clone(),
            Self::HttpBody(a) => a.default.clone(),
            Self::HttpFormData(a) => a.default.clone(),
            Self::HttpBodyRaw(a) => a.default.clone(),
            Self::HttpPath(a) => a.default.clone(),
        };

        default_attr.map(super::HttpInputDefaultValue::new)
    }

    pub fn to_src_token_stream(&self) -> proc_macro2::TokenStream {
        let http_parameter_input_src = crate::consts::get_http_parameter_input_src();
        match self {
            Self::HttpQuery(_) => quote::quote!(#http_parameter_input_src::Query),
            Self::HttpPath(_) => quote::quote!(#http_parameter_input_src::Path),
            Self::HttpHeader(_) => quote::quote!(#http_parameter_input_src::Header),
            Self::HttpBody(_) => quote::quote!(#http_parameter_input_src::BodyModel),
            Self::HttpFormData(_) => quote::quote!(#http_parameter_input_src::FormData),
            Self::HttpBodyRaw(_) => quote:: quote!(#http_parameter_input_src::BodyRaw),
        }
    }

    pub fn description(&'s self) -> &'s str {
        match self {
            Self::HttpHeader(http_header) => http_header.description,
            Self::HttpQuery(http_query) => http_query.description,
            Self::HttpBody(http_body) => http_body.description,
            Self::HttpFormData(http_form_data) => http_form_data.description,
            Self::HttpBodyRaw(http_body_raw) => http_body_raw.description,
            Self::HttpPath(http_path) => http_path.description,
        }
    }

    pub fn get_name(&'s self) -> Option<&'s str> {
        match self {
            Self::HttpHeader(http_header) => http_header.name,
            Self::HttpQuery(http_query) => http_query.name,
            Self::HttpBody(http_body) => http_body.name,
            Self::HttpFormData(http_form_data) => http_form_data.name,
            Self::HttpBodyRaw(http_body_raw) => http_body_raw.name,
            Self::HttpPath(http_path) => http_path.name,
        }
    }

    /// Name of a validator function, applied to the outgoing value before it is sent.
    pub fn validator(&'s self) -> Option<&'s str> {
        match self {
            Self::HttpHeader(a) => a.validator,
            Self::HttpQuery(a) => a.validator,
            Self::HttpBody(a) => a.validator,
            Self::HttpFormData(a) => a.validator,
            Self::HttpBodyRaw(a) => a.validator,
            Self::HttpPath(a) => a.validator,
        }
    }

    pub fn has_trim(&self) -> bool {
        match self {
            Self::HttpHeader(a) => a.trim,
            Self::HttpQuery(a) => a.trim,
            Self::HttpBody(a) => a.trim,
            Self::HttpFormData(a) => a.trim,
            Self::HttpBodyRaw(a) => a.trim,
            Self::HttpPath(a) => a.trim,
        }
    }

    pub fn has_to_lowercase(&self) -> bool {
        match self {
            Self::HttpHeader(a) => a.to_lowercase,
            Self::HttpQuery(a) => a.to_lowercase,
            Self::HttpBody(a) => a.to_lowercase,
            Self::HttpFormData(a) => a.to_lowercase,
            Self::HttpBodyRaw(a) => a.to_lowercase,
            Self::HttpPath(a) => a.to_lowercase,
        }
    }

    pub fn has_to_uppercase(&self) -> bool {
        match self {
            Self::HttpHeader(a) => a.to_uppercase,
            Self::HttpQuery(a) => a.to_uppercase,
            Self::HttpBody(a) => a.to_uppercase,
            Self::HttpFormData(a) => a.to_uppercase,
            Self::HttpBodyRaw(a) => a.to_uppercase,
            Self::HttpPath(a) => a.to_uppercase,
        }
    }

    pub fn has_print_request_to_console(&self) -> bool {
        match self {
            Self::HttpHeader(a) => a.print_request_to_console,
            Self::HttpQuery(a) => a.print_request_to_console,
            Self::HttpBody(a) => a.print_request_to_console,
            Self::HttpFormData(a) => a.print_request_to_console,
            Self::HttpBodyRaw(a) => a.print_request_to_console,
            Self::HttpPath(a) => a.print_request_to_console,
        }
    }
}

impl<'s> From<HttpHeaderAttribute<'s>> for HttpFieldAttribute<'s> {
    fn from(value: HttpHeaderAttribute<'s>) -> Self {
        HttpFieldAttribute::HttpHeader(value)
    }
}

impl<'s> From<HttpQueryAttribute<'s>> for HttpFieldAttribute<'s> {
    fn from(value: HttpQueryAttribute<'s>) -> Self {
        HttpFieldAttribute::HttpQuery(value)
    }
}

impl<'s> From<HttpBodyAttribute<'s>> for HttpFieldAttribute<'s> {
    fn from(value: HttpBodyAttribute<'s>) -> Self {
        HttpFieldAttribute::HttpBody(value)
    }
}

impl<'s> From<HttpFormDataAttribute<'s>> for HttpFieldAttribute<'s> {
    fn from(value: HttpFormDataAttribute<'s>) -> Self {
        HttpFieldAttribute::HttpFormData(value)
    }
}

impl<'s> From<HttpBodyRawAttribute<'s>> for HttpFieldAttribute<'s> {
    fn from(value: HttpBodyRawAttribute<'s>) -> Self {
        HttpFieldAttribute::HttpBodyRaw(value)
    }
}

impl<'s> From<HttpPathAttribute<'s>> for HttpFieldAttribute<'s> {
    fn from(value: HttpPathAttribute<'s>) -> Self {
        HttpFieldAttribute::HttpPath(value)
    }
}
