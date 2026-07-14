use std::str::FromStr;

use proc_macro2::TokenStream;
use quote::quote;
use types_reader::PropertyType;

use super::{http_input_props::HttpInputProperties, InputField};

/// Emits `impl my_http_utils::schema::client::THttpRequestBuilder for #struct_name`.
///
/// The model pushes its fields straight into the concrete `my_http_utils::UrlBuilder` (path +
/// query) and into a `HeaderBuilder` sink, and hands over its body as `HttpRequestBody`. Per
/// field, the outgoing value gets the model's directives applied — `trim`, then
/// `to_lowercase`/`to_uppercase`, then `validator`, then `print_request_to_console` — before
/// it is sent. Everything is wasm-safe.
pub fn generate_client_writer(
    struct_name: &syn::Ident,
    props: &HttpInputProperties,
) -> Result<TokenStream, syn::Error> {
    // All three methods are always emitted (the trait has no defaults), empty where the model
    // has no such fields — so every model concretely implements the whole trait.

    // ---- fill_url: path segments (declaration order) then query params ----
    let mut url_stmts = Vec::new();
    if let Some(fields) = &props.path_fields {
        for field in fields {
            url_stmts.push(field_pushes(field, Sink::Path)?);
        }
    }
    if let Some(fields) = &props.query_string_fields {
        for field in fields {
            url_stmts.push(field_pushes(field, Sink::Query)?);
        }
    }
    let fill_url = if url_stmts.is_empty() {
        quote! {
            fn fill_url(&self, _url: &mut my_http_utils::UrlBuilder) -> Result<(), my_http_utils::schema::client::HttpRequestBuildError> {
                Ok(())
            }
        }
    } else {
        quote! {
            fn fill_url(&self, __url: &mut my_http_utils::UrlBuilder) -> Result<(), my_http_utils::schema::client::HttpRequestBuildError> {
                #(#url_stmts)*
                Ok(())
            }
        }
    };

    // ---- fill_headers ----
    let mut hdr_stmts = Vec::new();
    if let Some(fields) = &props.header_fields {
        for field in fields {
            hdr_stmts.push(field_pushes(field, Sink::Header)?);
        }
    }
    let fill_headers = if hdr_stmts.is_empty() {
        quote! {
            fn fill_headers(&self, _h: &mut impl my_http_utils::schema::client::HeaderBuilder) -> Result<(), my_http_utils::schema::client::HttpRequestBuildError> {
                Ok(())
            }
        }
    } else {
        quote! {
            fn fill_headers(&self, __h: &mut impl my_http_utils::schema::client::HeaderBuilder) -> Result<(), my_http_utils::schema::client::HttpRequestBuildError> {
                #(#hdr_stmts)*
                Ok(())
            }
        }
    };

    // ---- get_body (always emitted; Empty when the model has no body) ----
    let get_body = generate_get_body(props)?;

    Ok(quote! {
        impl my_http_utils::schema::client::THttpRequestBuilder for #struct_name {
            #fill_url
            #fill_headers
            #get_body
        }
    })
}

#[derive(Clone, Copy)]
enum Sink {
    Path,
    Query,
    Header,
    FormData,
}

impl Sink {
    /// Emit the sink call for a `&str` value expression `#v`.
    fn call(&self, name: &str, v: &TokenStream) -> TokenStream {
        match self {
            Sink::Path => quote!(__url.append_path_segment(#v);),
            Sink::Query => quote!(__url.append_query_param(#name, Some(#v));),
            Sink::Header => quote!(__h.add_header(#name, #v);),
            Sink::FormData => quote!(__fd = __fd.append_form_data_field(#name, #v);),
        }
    }
}

/// Emits the push(es) for one field, handling `Option` (only when `Some`) and `Vec` (each).
fn field_pushes(field: &InputField, sink: Sink) -> Result<TokenStream, syn::Error> {
    let name = field.get_input_field_name()?;
    let ident = field.property.get_field_name_ident();

    match &field.property.ty {
        PropertyType::OptionOf(inner) => {
            let one = one_value(field, sink, name, quote!(value), inner)?;
            Ok(quote! {
                if let Some(value) = &self.#ident {
                    #one
                }
            })
        }
        PropertyType::VecOf(inner) => {
            let one = one_value(field, sink, name, quote!(value), inner)?;
            Ok(quote! {
                for value in &self.#ident {
                    #one
                }
            })
        }
        other => one_value(field, sink, name, quote!(self.#ident), other),
    }
}

/// One value push, with the model's outgoing directives applied. When no directive is set the
/// value is pushed inline (no allocation); otherwise it is materialized into `__v` and
/// transformed / validated / printed first.
fn one_value(
    field: &InputField,
    sink: Sink,
    name: &str,
    place: TokenStream,
    ty: &PropertyType,
) -> Result<TokenStream, syn::Error> {
    let trim = field.attr.has_trim();
    let lower = field.attr.has_to_lowercase();
    let upper = field.attr.has_to_uppercase();
    let validator = field.attr.validator();
    let print = field.attr.has_print_request_to_console();

    if !trim && !lower && !upper && validator.is_none() && !print {
        let v = value_ref_expr(place, ty);
        return Ok(sink.call(name, &v));
    }

    // Transform chain -> owned String in `__v`.
    let mut chain = value_base_expr(place, ty);
    if trim {
        chain = quote!(#chain.trim());
    }
    if lower {
        chain = quote!(#chain.to_lowercase());
    } else if upper {
        chain = quote!(#chain.to_uppercase());
    } else {
        chain = quote!(#chain.to_string());
    }

    let validate = if let Some(validator) = validator {
        let validator = TokenStream::from_str(validator).map_err(|e| {
            syn::Error::new_spanned(field.property.field, format!("Invalid validator: {}", e))
        })?;
        quote! {
            if let Err(__err) = #validator(__v.as_str()) {
                return Err(my_http_utils::schema::client::HttpRequestBuildError::new(#name, __err));
            }
        }
    } else {
        quote!()
    };

    let print_stmt = if print {
        quote!(println!("{} = {}", #name, __v);)
    } else {
        quote!()
    };

    let push = sink.call(name, &quote!(__v.as_str()));

    Ok(quote! {
        {
            let __v = #chain;
            #validate
            #print_stmt
            #push
        }
    })
}

/// A `&str` value expression for the inline (no-directive) push.
fn value_ref_expr(expr: TokenStream, ty: &PropertyType) -> TokenStream {
    match ty {
        PropertyType::String => quote!(#expr.as_str()),
        PropertyType::Struct(..) => quote!(#expr.as_str()),
        PropertyType::DateTime => quote!(&#expr.to_rfc3339()),
        _ => quote!(&#expr.to_string()),
    }
}

/// A `&str`/`String` base expression usable as the start of a transform chain (no leading `&`).
fn value_base_expr(expr: TokenStream, ty: &PropertyType) -> TokenStream {
    match ty {
        PropertyType::String => quote!(#expr.as_str()),
        PropertyType::Struct(..) => quote!(#expr.as_str()),
        PropertyType::DateTime => quote!(#expr.to_rfc3339()),
        _ => quote!(#expr.to_string()),
    }
}

/// Builds the required `get_body` method. Exactly one of body / form-data / body-raw can be
/// present (the derive's `self_check` forbids mixing); when the model has no body it returns
/// `HttpRequestBody::Empty`. Always emits a method (the trait has no default).
fn generate_get_body(props: &HttpInputProperties) -> Result<TokenStream, syn::Error> {
    if let Some(fields) = &props.body_fields {
        let mut inserts = Vec::with_capacity(fields.len());
        for field in fields {
            let name = field.get_input_field_name()?;
            let ident = field.property.get_field_name_ident();
            if let PropertyType::OptionOf(_) = &field.property.ty {
                inserts.push(quote! {
                    if let Some(value) = &self.#ident {
                        __obj.insert(#name.to_string(), serde_json::to_value(value).unwrap_or(serde_json::Value::Null));
                    }
                });
            } else {
                inserts.push(quote! {
                    __obj.insert(#name.to_string(), serde_json::to_value(&self.#ident).unwrap_or(serde_json::Value::Null));
                });
            }
        }
        return Ok(quote! {
            #[allow(clippy::extra_unused_type_parameters)]
            fn get_body<__TRnd: my_http_utils::schema::client::RandomStringGenerator>(self) -> Result<my_http_utils::body::HttpRequestBody, my_http_utils::schema::client::HttpRequestBuildError> {
                let mut __obj = serde_json::Map::new();
                #(#inserts)*
                match serde_json::to_vec(&serde_json::Value::Object(__obj)) {
                    Ok(__bytes) => Ok(my_http_utils::body::HttpRequestBody::Json(__bytes)),
                    Err(_) => Ok(my_http_utils::body::HttpRequestBody::Empty),
                }
            }
        });
    }

    if let Some(fields) = &props.form_data_fields {
        let mut appends = Vec::with_capacity(fields.len());
        for field in fields {
            appends.push(field_pushes(field, Sink::FormData)?);
        }
        return Ok(quote! {
            fn get_body<__TRnd: my_http_utils::schema::client::RandomStringGenerator>(self) -> Result<my_http_utils::body::HttpRequestBody, my_http_utils::schema::client::HttpRequestBuildError> {
                // Multipart boundary: random suffix supplied by the caller's TRnd. A boundary
                // only has to be absent from this one request's content, so a fresh random
                // string per request suffices.
                let mut __fd = my_http_utils::body::FormDataBody::new(&__TRnd::generate_random_string(16));
                #(#appends)*
                Ok(my_http_utils::body::HttpRequestBody::FormData(__fd))
            }
        });
    }

    if let Some(field) = &props.body_raw_field {
        let ident = field.property.get_field_name_ident();
        let body = match &field.property.ty {
            PropertyType::VecOf(_) => quote! {
                my_http_utils::body::HttpRequestBody::Raw { data: self.#ident, content_type: None }
            },
            PropertyType::OptionOf(inner) => match inner.as_ref() {
                PropertyType::VecOf(_) => quote! {
                    match self.#ident {
                        Some(__d) => my_http_utils::body::HttpRequestBody::Raw { data: __d, content_type: None },
                        None => my_http_utils::body::HttpRequestBody::Empty,
                    }
                },
                _ => quote! {
                    match self.#ident {
                        Some(__v) => match serde_json::to_vec(&__v) {
                            Ok(__bytes) => my_http_utils::body::HttpRequestBody::Raw {
                                data: __bytes,
                                content_type: Some("application/json"),
                            },
                            Err(_) => my_http_utils::body::HttpRequestBody::Empty,
                        },
                        None => my_http_utils::body::HttpRequestBody::Empty,
                    }
                },
            },
            _ => quote! {
                match serde_json::to_vec(&self.#ident) {
                    Ok(__bytes) => my_http_utils::body::HttpRequestBody::Raw {
                        data: __bytes,
                        content_type: Some("application/json"),
                    },
                    Err(_) => my_http_utils::body::HttpRequestBody::Empty,
                }
            },
        };
        return Ok(quote! {
            #[allow(clippy::extra_unused_type_parameters)]
            fn get_body<__TRnd: my_http_utils::schema::client::RandomStringGenerator>(self) -> Result<my_http_utils::body::HttpRequestBody, my_http_utils::schema::client::HttpRequestBuildError> {
                Ok(#body)
            }
        });
    }

    // No body -> emit an Empty get_body (trait has no default).
    Ok(quote! {
        #[allow(clippy::extra_unused_type_parameters)]
        fn get_body<__TRnd: my_http_utils::schema::client::RandomStringGenerator>(self) -> Result<my_http_utils::body::HttpRequestBody, my_http_utils::schema::client::HttpRequestBuildError> {
            Ok(my_http_utils::body::HttpRequestBody::Empty)
        }
    })
}
