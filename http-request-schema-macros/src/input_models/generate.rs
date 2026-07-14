use proc_macro::TokenStream;

use quote::quote;

use super::http_input_props::HttpInputProperties;

pub fn generate(ast: &syn::DeriveInput, debug: &mut bool) -> Result<TokenStream, syn::Error> {
    let struct_name = &ast.ident;

    let fields = types_reader::StructProperty::read(ast)?;

    for prop in &fields {
        if prop.attrs.has_attr("debug") {
            *debug = true;
        }
    }

    let input_fields = HttpInputProperties::new(&fields)?;

    let http_input_param = crate::consts::get_http_input_parameter_with_ns();

    let http_input = match super::docs::generate_http_input(&input_fields) {
        Ok(result) => result,
        Err(err) => err.to_compile_error(),
    };

    let http_routes = match http_routes(&input_fields) {
        Ok(result) => {
            if result.is_empty() {
                quote! {None}
            } else {
                quote!(Some(vec![#(#result),*]))
            }
        }
        Err(err) => err.to_compile_error(),
    };

    let client_writer = match super::client_writer::generate_client_writer(struct_name, &input_fields)
    {
        Ok(result) => result,
        Err(err) => err.to_compile_error(),
    };

    // Server-side sync `parse` + `READS_BODY` — only under the `server` feature (see
    // `generate_parse_impl`); empty otherwise, keeping the client / wasm build lean.
    let parse_impl = match generate_parse_impl(struct_name, &input_fields) {
        Ok(result) => result,
        Err(err) => err.to_compile_error(),
    };

    // Schema description (`get_input_params` / `get_model_routes`) is an OpenAPI/Swagger concern
    // — server only. Browser clients get just the request builder, so their bundles stay small.
    let schema_impl = if cfg!(feature = "server") {
        quote! {
            impl #struct_name{
                pub fn get_input_params()->Vec<#http_input_param>{
                    #http_input
                }

                pub fn get_model_routes()->Option<Vec<&'static str>>{
                    #http_routes
                }
            }
        }
    } else {
        quote!()
    };

    // Three halves from the same markup: schema description (server), the client request builder
    // (`THttpRequestBuilder`, always), and — on the server — the sync `parse`.
    let result = quote! {
        #schema_impl

        #client_writer

        #parse_impl
    };
    Ok(result.into())
}

#[cfg(feature = "server")]
fn generate_parse_impl(
    struct_name: &syn::Ident,
    input_fields: &HttpInputProperties,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    super::parse::generate_parse(struct_name, input_fields)
}

#[cfg(not(feature = "server"))]
fn generate_parse_impl(
    _struct_name: &syn::Ident,
    _input_fields: &HttpInputProperties,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    Ok(quote!())
}

fn http_routes(props: &HttpInputProperties) -> Result<Vec<proc_macro2::TokenStream>, syn::Error> {
    let mut result = Vec::new();

    if let Some(path_fields) = &props.path_fields {
        for input_field in path_fields {
            let name = input_field.get_input_field_name()?;
            result.push(quote! {
                #name
            });
        }
    }

    Ok(result)
}
