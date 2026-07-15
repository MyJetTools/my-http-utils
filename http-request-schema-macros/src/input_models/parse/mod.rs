//! Generates the server-independent sync `parse` (and the `READS_BODY` const) on a `MyHttpInput`
//! model. Only built with the `server` feature. Semantics mirror the old server-side
//! `parse_http_input` codegen (`my-http-server-macros`) 1:1, but every source is read through the
//! abstract [`my_http_utils::http_input::core::THttpRequest`], and values convert via
//! `HttpInputValue::try_into` instead of `EncodedParamValue`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;
use types_reader::PropertyType;

use super::http_input_props::HttpInputProperties;
use super::InputField;

pub fn generate_parse(
    name: &Ident,
    props: &HttpInputProperties,
) -> Result<TokenStream, syn::Error> {
    let mut fields_to_return = Vec::new();
    let mut reads = Vec::new();
    let mut validations = Vec::new();

    // ---- path (always required; Option is rejected by self_check) ----
    if let Some(path_fields) = &props.path_fields {
        for field in path_fields {
            fields_to_return.push(field.read_value_with_transformation()?);
            if let Some(validator) = field.get_validator_as_token_stream() {
                validations.push(validator);
            }
            reads.push(read_path(field)?);
        }
    }

    // ---- headers ----
    if let Some(header_fields) = &props.header_fields {
        for field in header_fields {
            fields_to_return.push(field.read_value_with_transformation()?);
            if let Some(validator) = field.get_validator_as_token_stream() {
                validations.push(validator);
            }
            reads.push(read_header(field)?);
        }
    }

    // ---- query string ----
    if let Some(query_fields) = &props.query_string_fields {
        reads.push(quote! {
            let __query = my_http_utils::http_input::core::QueryStringReader::new(request.get_query_string())?;
        });
        for field in query_fields {
            fields_to_return.push(field.read_value_with_transformation()?);
            if let Some(validator) = field.get_validator_as_token_stream() {
                validations.push(validator);
            }
            reads.push(read_query(field)?);
        }
    }

    // ---- body: json / form-data (named fields) and/or raw (whole body) ----
    // READS_BODY is true whenever the model touches the body at all.
    let reads_body = props.body_fields.is_some()
        || props.form_data_fields.is_some()
        || props.body_raw_field.is_some();

    // The parsing `BodyReader` (content-type dispatch) is only needed for reading NAMED body
    // fields — json / form-data, or an Option `#[http_body_raw]` (which reads a named field).
    // A non-Option `#[http_body_raw]` takes the whole body verbatim via `read_raw_body` and must
    // NOT go through the reader, whose eager JSON/url-encoded parse would reject a non-object /
    // malformed / binary body that the field's own `TryInto` handles fine.
    let body_raw_is_option = props
        .body_raw_field
        .as_ref()
        .map(|f| f.property.ty.is_option())
        .unwrap_or(false);
    let needs_body_reader =
        props.body_fields.is_some() || props.form_data_fields.is_some() || body_raw_is_option;

    if needs_body_reader {
        reads.push(quote! {
            let __body = my_http_utils::http_input::core::BodyReader::from_parts(
                request.get_body(),
                request.get_content_type(),
            )?;
        });
    }

    if let Some(body_fields) = &props.body_fields {
        for field in body_fields {
            fields_to_return.push(field.read_value_with_transformation()?);
            if let Some(validator) = field.get_validator_as_token_stream() {
                validations.push(validator);
            }
            reads.push(read_body(field)?);
        }
    }

    if let Some(form_data_fields) = &props.form_data_fields {
        for field in form_data_fields {
            fields_to_return.push(field.read_value_with_transformation()?);
            if let Some(validator) = field.get_validator_as_token_stream() {
                validations.push(validator);
            }
            reads.push(read_body(field)?);
        }
    }

    // Raw body reads inline into the struct literal (no local, no transformation) — matching the
    // original codegen.
    if let Some(raw_field) = &props.body_raw_field {
        fields_to_return.push(read_body_raw(raw_field)?);
    }

    Ok(quote! {
        impl #name {
            /// `true` when this model reads the request body (`http_body` / `http_body_raw` /
            /// `http_form_data`). The server uses it to avoid reading the body when it is not
            /// needed before calling [`Self::parse`].
            pub const READS_BODY: bool = #reads_body;

            /// Parses the model out of an abstract [`my_http_utils::http_input::core::THttpRequest`].
            /// Synchronous: the body is expected to be already received (see [`Self::READS_BODY`]).
            pub fn parse(
                request: &impl my_http_utils::http_input::core::THttpRequest,
            ) -> Result<Self, my_http_utils::http_input::HttpParseError> {
                #(#reads)*
                #(#validations)*
                Ok(#name { #(#fields_to_return),* })
            }
        }
    })
}

fn read_path(field: &InputField) -> Result<TokenStream, syn::Error> {
    let name = field.get_input_field_name()?;
    let let_param = field.get_let_input_param();
    Ok(quote! {
        let #let_param = my_http_utils::http_input::core::read_path_value(request, #name)?.try_into()?;
    })
}

fn read_header(field: &InputField) -> Result<TokenStream, syn::Error> {
    let name = field.get_input_field_name()?;
    let let_param = field.get_let_input_param();

    if field.property.ty.is_option() {
        let default_value = field.get_default_value_opt_case()?;
        return Ok(quote! {
            let #let_param = if let Some(value) = my_http_utils::http_input::core::read_header_optional(request, #name) {
                Some(value.try_into()?)
            } else {
                #default_value
            };
        });
    }

    if !field.has_default_value() {
        return Ok(quote! {
            let #let_param = my_http_utils::http_input::core::read_header_required(request, #name)?.try_into()?;
        });
    }

    let default_value = field.get_default_value_non_opt_case()?;
    Ok(quote! {
        let #let_param = if let Some(value) = my_http_utils::http_input::core::read_header_optional(request, #name) {
            value.try_into()?
        } else {
            #default_value
        };
    })
}

fn read_query(field: &InputField) -> Result<TokenStream, syn::Error> {
    let name = field.get_input_field_name()?;

    match &field.property.ty {
        PropertyType::OptionOf(sub_ty) => {
            verify_default_value(field, sub_ty)?;
            let default_value = field.get_default_value_opt_case()?;
            let let_param = field.get_let_input_param();
            Ok(quote! {
                let #let_param = if let Some(value) = __query.get_optional(#name) {
                    Some(value.try_into()?)
                } else {
                    #default_value
                };
            })
        }
        PropertyType::VecOf(_) => {
            let ident = field.property.get_field_name_ident();
            Ok(quote! {
                let #ident = {
                    let items = __query.get_vec(#name)?;
                    let mut result = Vec::with_capacity(items.len());
                    for value in items {
                        result.push(value.try_into()?);
                    }
                    result
                };
            })
        }
        PropertyType::Struct(..) => read_struct_with_optional_default(field, quote!(__query)),
        _ => {
            verify_default_value(field, &field.property.ty)?;
            if field.has_default_value() {
                let default_value = field.get_default_value_non_opt_case()?;
                let let_param = field.get_let_input_param();
                return Ok(quote! {
                    let #let_param = match __query.get_optional(#name) {
                        Some(value) => value.try_into()?,
                        None => #default_value,
                    };
                });
            }
            read_required(field, quote!(__query))
        }
    }
}

fn read_body(field: &InputField) -> Result<TokenStream, syn::Error> {
    let name = field.get_input_field_name()?;

    match &field.property.ty {
        PropertyType::OptionOf(sub_ty) => {
            verify_default_value(field, sub_ty)?;
            let default_value = field.get_default_value_opt_case()?;
            let let_param = field.get_let_input_param();
            Ok(quote! {
                let #let_param = if let Some(value) = __body.get_optional(#name) {
                    Some(value.try_into()?)
                } else {
                    #default_value
                };
            })
        }
        PropertyType::Struct(..) => read_struct_with_optional_default(field, quote!(__body)),
        _ => {
            verify_default_value(field, &field.property.ty)?;
            if field.has_default_value() {
                let default_value = field.get_default_value_non_opt_case()?;
                let let_param = field.get_let_input_param();
                return Ok(quote! {
                    let #let_param = match __body.get_optional(#name) {
                        Some(value) => value.try_into()?,
                        None => #default_value,
                    };
                });
            }
            read_required(field, quote!(__body))
        }
    }
}

fn read_body_raw(field: &InputField) -> Result<TokenStream, syn::Error> {
    let ident = field.property.get_field_name_ident();
    if field.property.ty.is_option() {
        let name = field.get_input_field_name()?;
        Ok(quote! {
            #ident: if let Some(value) = __body.get_optional(#name) {
                Some(value.try_into()?)
            } else {
                None
            }
        })
    } else {
        // Non-Option: the whole body, verbatim (no content-type parsing). `read_raw_body` returns
        // the raw `Vec<u8>`; the field type builds itself from those bytes via the crate-local
        // `FromRawBody` — `Vec<u8>` = the bytes as-is, `RawData` / `RawDataTyped` = verbatim (the
        // JSON error, if any, is deferred to `RawDataTyped::deserialize_json`), `String` = a utf-8
        // check. `FromRawBody` (not `TryFrom<Vec<u8>>`) keeps std's `From` free for the client-side
        // `From<T>` on `RawDataTyped<T>`. Byte source, so a raw body is never mis-routed via JSON.
        Ok(quote!(#ident: my_http_utils::http_input::core::FromRawBody::from_raw_body(
            my_http_utils::http_input::core::read_raw_body(request)
        )?))
    }
}

/// Struct/enum field: only a bare `default` (→ `create_default()`) is allowed; otherwise it is
/// required.
fn read_struct_with_optional_default(
    field: &InputField,
    data_src: TokenStream,
) -> Result<TokenStream, syn::Error> {
    let name = field.get_input_field_name()?;
    if let Some(default_value) = field.attr.get_default() {
        if !default_value.has_empty_value() {
            return Err(syn::Error::new_spanned(
                field.property.get_field_name_ident(),
                "Please use `default` with no value. A Struct/Enum should implement `create_default`, and the default is read from there",
            ));
        }
        let default_value = field.get_default_value_opt_case()?;
        let let_param = field.get_let_input_param();
        return Ok(quote! {
            let #let_param = match #data_src.get_optional(#name) {
                Some(value) => value.try_into()?,
                None => #default_value,
            };
        });
    }

    read_required(field, data_src)
}

fn read_required(field: &InputField, data_src: TokenStream) -> Result<TokenStream, syn::Error> {
    let name = field.get_input_field_name()?;
    let ident = field.property.get_field_name_ident();
    let ty = field.property.ty.get_token_stream();
    Ok(quote! {
        let #ident: #ty = #data_src.get_required(#name)?.try_into()?;
    })
}

/// Rejects `default` values whose shape does not match the field type (mirrors the server's
/// `verify_default_value`): struct/enum accept only a bare `default`, everything else needs a
/// value.
fn verify_default_value(field: &InputField, ty: &PropertyType) -> Result<(), syn::Error> {
    let empty_only = matches!(ty, PropertyType::Struct(_, _));

    match field.attr.get_default() {
        None => Ok(()),
        Some(default_value) => {
            if empty_only && !default_value.has_empty_value() {
                return Err(syn::Error::new_spanned(
                    field.property.get_field_name_ident(),
                    "Please use `default` with no value for a Struct/Enum field",
                ));
            }
            if !empty_only && default_value.has_empty_value() {
                return Err(syn::Error::new_spanned(
                    field.property.get_field_name_ident(),
                    "Please use `default` with a value",
                ));
            }
            Ok(())
        }
    }
}
