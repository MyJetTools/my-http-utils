//! Generates the server-independent sync `parse` (and the `READS_BODY` const) on a `MyHttpInput`
//! model, plus its async twin `parse_with_body_stream`, which reads the body out of a
//! `rust_extensions::AsyncBytesStream` instead. Only built with the `server` feature. Semantics
//! mirror the old server-side `parse_http_input` codegen (`my-http-server-macros`) 1:1, but every
//! source is read through the abstract [`my_http_utils::http_input::core::THttpRequest`], and
//! values convert via `HttpInputValue::try_into` instead of `EncodedParamValue`.
//!
//! The two share everything but the body: path / header / query reads, the body field reads off
//! `__body`, the validators and the struct literal are the same tokens. Only where `__body` comes
//! from, and how a whole-body (`#[http_body_raw]`) or streamed (`#[http_body_as_stream]`) field is
//! filled, differ.

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
    // READS_BODY is true whenever the model needs the body MATERIALISED before `parse` runs.
    // `#[http_body_as_stream]` deliberately does NOT count: the body must not be both
    // materialised and streamed, so a streaming model reports READS_BODY = false and
    // STREAMS_BODY = true instead.
    let reads_body = props.body_fields.is_some()
        || props.form_data_fields.is_some()
        || props.body_raw_field.is_some();

    let streams_body = props.body_as_stream_field.is_some();

    // READS_BODY_RAW narrows READS_BODY down to `#[http_body_raw]` — a body taken as it is rather
    // than read field by field. The server reads every other body through
    // `parse_with_body_stream`, and this one materialised, as `parse` reads it.
    let reads_body_raw = props.body_raw_field.is_some();

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

    // Read off `__body` by both `parse` and `parse_with_body_stream`.
    let mut body_reads = Vec::new();
    // The names they are read by — what a body that comes as a stream keeps of itself.
    let mut body_names = Vec::new();

    if let Some(body_fields) = &props.body_fields {
        for field in body_fields {
            fields_to_return.push(field.read_value_with_transformation()?);
            if let Some(validator) = field.get_validator_as_token_stream() {
                validations.push(validator);
            }
            body_names.push(field.get_input_field_name()?);
            body_reads.push(read_body(field)?);
        }
    }

    if let Some(form_data_fields) = &props.form_data_fields {
        for field in form_data_fields {
            fields_to_return.push(field.read_value_with_transformation()?);
            if let Some(validator) = field.get_validator_as_token_stream() {
                validations.push(validator);
            }
            body_names.push(field.get_input_field_name()?);
            body_reads.push(read_body(field)?);
        }
    }

    // Where `__body` comes from, and the body-taking field of the struct literal: the request's
    // own body for `parse`, the stream for `parse_with_body_stream`.
    let mut sync_body = Vec::new();
    let mut stream_body = Vec::new();
    let mut sync_body_field = None;
    let mut stream_body_field = None;

    // Raw body reads inline into the struct literal (no local, no transformation) — matching the
    // original codegen.
    if let Some(raw_field) = &props.body_raw_field {
        let ident = raw_field.property.get_field_name_ident();

        if raw_field.property.ty.is_option() {
            body_names.push(raw_field.get_input_field_name()?);
            let field = read_body_raw_optional(raw_field)?;
            sync_body_field = Some(field.clone());
            stream_body_field = Some(field);
        } else {
            // Non-Option: the whole body, verbatim (no content-type parsing). The field type
            // builds itself from those bytes via the crate-local `FromRawBody` — `Vec<u8>` = the
            // bytes as-is, `RawData` / `RawDataTyped` = verbatim (the JSON error, if any, is
            // deferred to `RawDataTyped::deserialize_json`), `String` = a utf-8 check. `FromRawBody`
            // (not `TryFrom<Vec<u8>>`) keeps std's `From` free for the client-side `From<T>` on
            // `RawDataTyped<T>`. Byte source, so a raw body is never mis-routed via JSON.
            sync_body_field = Some(quote! {
                #ident: my_http_utils::http_input::core::FromRawBody::from_raw_body(
                    my_http_utils::http_input::core::read_raw_body(request)
                )?
            });
            stream_body_field = Some(quote! {
                #ident: my_http_utils::http_input::core::FromRawBody::from_raw_body(
                    my_http_utils::http_input::core::read_raw_body_from_stream(body).await?
                )?
            });
        }
    }

    if needs_body_reader {
        sync_body.push(quote! {
            let __body = my_http_utils::http_input::core::BodyReader::from_parts(
                request.get_body(),
                request.get_content_type(),
            )?;
        });
        stream_body.push(quote! {
            let __body_from_stream = my_http_utils::http_input::core::BodyFromStream::read(
                body,
                request.get_content_type(),
                &[#(#body_names),*],
            )
            .await?;
            let __body = __body_from_stream.get_body_reader()?;
        });
    }

    // `parse`: the stream is created and already being filled by the transport BEFORE `parse`
    // runs, so `parse` only moves the ready `HttpBodyAsStream` into the field.
    // `parse_with_body_stream`: the stream itself goes into the field — nothing is read out of it.
    // Either way inline into the struct literal, like the raw body, and no `BodyReader` is built.
    if let Some(stream_field) = &props.body_as_stream_field {
        let ident = stream_field.property.get_field_name_ident();
        sync_body_field = Some(quote! {
            #ident: request.take_body_stream().ok_or_else(||
                my_http_utils::http_input::HttpParseError::BodyStream(
                    "Body stream is not available".to_string()))?
        });
        stream_body_field = Some(quote! {
            #ident: my_http_utils::http_input::HttpBodyAsStream::from_bytes_stream(body)
        });
    }

    if !reads_body && !streams_body {
        // The model takes nothing from the body, so the stream is let go unread.
        stream_body.push(quote!(drop(body);));
    }

    let sync_body_field = sync_body_field.into_iter();
    let stream_body_field = stream_body_field.into_iter();

    Ok(quote! {
        impl #name {
            /// `true` when this model reads the request body (`http_body` / `http_body_raw` /
            /// `http_form_data`). The server uses it to avoid reading the body when it is not
            /// needed before calling [`Self::parse`].
            pub const READS_BODY: bool = #reads_body;

            /// `true` when this model takes the body as a stream (`#[http_body_as_stream]`).
            /// Mutually exclusive with [`Self::READS_BODY`] — the body cannot be both
            /// materialised and streamed. Emitted for **every** model (`false` for the rest), so
            /// the server codegen that reads it always compiles.
            pub const STREAMS_BODY: bool = #streams_body;

            /// `true` when this model has a `#[http_body_raw]` field — the body is read for what
            /// it is, not for named fields. Implies [`Self::READS_BODY`]. Emitted for **every**
            /// model (`false` for the rest), like [`Self::STREAMS_BODY`].
            pub const READS_BODY_RAW: bool = #reads_body_raw;

            /// Parses the model out of an abstract [`my_http_utils::http_input::core::THttpRequest`].
            /// Synchronous: the body is expected to be already received (see [`Self::READS_BODY`]).
            pub fn parse(
                request: &impl my_http_utils::http_input::core::THttpRequest,
            ) -> Result<Self, my_http_utils::http_input::HttpParseError> {
                #(#reads)*
                #(#sync_body)*
                #(#body_reads)*
                #(#validations)*
                Ok(#name { #(#fields_to_return,)* #(#sync_body_field)* })
            }

            /// Parses the model the way [`Self::parse`] does, with the body read out of `body` —
            /// the request's `get_body` / `take_body_stream` are not called. Path, headers and
            /// query are read first, so a request that fails on them does not wait for its body.
            ///
            /// * Named body fields: a JSON body is read member by member as it arrives, and only
            ///   the members the model names are kept; any other body is read whole.
            /// * `#[http_body_raw]`: the whole body.
            /// * `#[http_body_as_stream]`: `body` itself goes into the field, unread.
            /// * No body fields: `body` is dropped unread.
            ///
            /// The bounds are the same for every model, so a caller that only knows the type name
            /// can call it.
            pub async fn parse_with_body_stream<TStream, TError>(
                request: &impl my_http_utils::http_input::core::THttpRequest,
                body: TStream,
            ) -> Result<Self, my_http_utils::http_input::HttpParseError>
            where
                TStream: my_http_utils::rust_extensions::AsyncBytesStream<TError>
                    + Send
                    + Sync
                    + 'static,
                TError: Into<my_http_utils::http_input::HttpParseError> + 'static,
            {
                #(#reads)*
                #(#stream_body)*
                #(#body_reads)*
                #(#validations)*
                Ok(#name { #(#fields_to_return,)* #(#stream_body_field)* })
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

/// An `Option` `#[http_body_raw]` reads a *named* body field (the non-Option one takes the whole
/// body — see `generate_parse`).
fn read_body_raw_optional(field: &InputField) -> Result<TokenStream, syn::Error> {
    let ident = field.property.get_field_name_ident();
    let name = field.get_input_field_name()?;
    Ok(quote! {
        #ident: if let Some(value) = __body.get_optional(#name) {
            Some(value.try_into()?)
        } else {
            None
        }
    })
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
