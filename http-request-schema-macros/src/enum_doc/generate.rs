use std::str::FromStr;

use proc_macro::TokenStream;
use types_reader::EnumCase;

use crate::enum_doc::enum_json::EnumJson;

use super::generate_default::generate_default_as_str_fn;

pub fn generate(ast: &syn::DeriveInput, as_integer: bool) -> Result<TokenStream, syn::Error> {
    let struct_name = &ast.ident;
    let struct_name_as_str = struct_name.to_string();

    let src_fields = EnumCase::read(ast)?;

    let mut fields = Vec::new();

    let mut default_str_value = None;
    let mut default_case_value = None;

    for src_field in src_fields {
        let enum_json = EnumJson::new(src_field)?;
        if enum_json.attr.default {
            default_str_value = Some(enum_json.get_enum_case_str_value()?);
            default_case_value = Some(enum_json.get_enum_case_value());
        }

        fields.push(enum_json);
    }

    // Default trait, from the case marked `default` (if any) — pure, no transport.
    let default_trait = if let Some(default_case) = &default_case_value {
        let default_case = proc_macro2::TokenStream::from_str(default_case).unwrap();

        quote::quote! {
            impl std::default::Default for #struct_name{
                fn default() -> Self {
                    Self::#default_case
                }
            }
        }
    } else {
        quote::quote!()
    };

    let use_documentation = crate::consts::get_use_documentation();

    let enum_cases = generate_enum_cases(&fields)?;

    let default_as_str_fn = generate_default_as_str_fn(default_str_value.as_ref());

    let enum_type = if as_integer {
        quote::quote!(EnumType::Integer)
    } else {
        quote::quote!(EnumType::String)
    };

    let enum_as_str = generate_enum_as_str(&fields)?;

    // Server-side conversion from a parsed request value into the enum — only emitted with the
    // `server` feature (it references `my_http_utils::http_input`, which is server-gated).
    let try_from_input = if cfg!(feature = "server") {
        generate_try_from_input(struct_name, &fields)?
    } else {
        quote::quote!()
    };

    // Schema description of the enum — OpenAPI/Swagger, server only.
    let data_type_provider = if cfg!(feature = "server") {
        quote::quote! {
            impl my_http_utils::schema::data_types::DataTypeProvider for #struct_name {
                fn get_data_type() -> my_http_utils::schema::data_types::HttpDataType {
                    #use_documentation;

                    let mut __es = data_types::HttpEnumStructure{
                        struct_id: #struct_name_as_str,
                        enum_type: #enum_type,
                        cases: vec![],
                    };

                    #(#enum_cases)*

                    __es.into_http_data_type_object()
                }

                fn get_generic_type() -> Option<String> {
                    None
                 }
            }
        }
    } else {
        quote::quote!()
    };

    let result = quote::quote! {
        // value -> string. The client request builder uses `as_str()` to serialize an
        // enum-typed field into a request.
        impl #struct_name{
            pub fn as_str(&self) -> &'static str{
                match self{
                    #(#enum_as_str)*
                }
            }

            #default_as_str_fn
        }

        #default_trait

        #data_type_provider

        #try_from_input
    };

    Ok(result.into())
}

/// Emits `TryFrom<HttpInputValue>`: reads the value as a string and matches it against each
/// case's string form (the same string `as_str` emits, so client→server round-trips) plus its
/// numeric `id`. Only *emitted* with the `server` feature (the fn itself just builds tokens, so
/// it always compiles).
fn generate_try_from_input(
    struct_name: &syn::Ident,
    cases: &[EnumJson],
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let mut arms = Vec::with_capacity(cases.len());

    for case in cases {
        let variant = case.src.get_name_ident();
        let str_value = case.get_enum_case_str_value()?;

        let mut patterns = vec![str_value.clone()];
        if let Some(id) = case.attr.id.as_ref() {
            if id != &str_value {
                patterns.push(id.clone());
            }
        }
        let patterns = patterns.iter().map(|p| quote::quote!(#p));

        arms.push(quote::quote! {
            #(#patterns)|* => Ok(Self::#variant),
        });
    }

    Ok(quote::quote! {
        impl<'s> std::convert::TryFrom<my_http_utils::http_input::HttpInputValue<'s>> for #struct_name {
            type Error = my_http_utils::http_input::HttpParseError;
            fn try_from(
                __value: my_http_utils::http_input::HttpInputValue<'s>,
            ) -> Result<Self, Self::Error> {
                let __s = __value.as_string()?;
                match __s.as_str() {
                    #(#arms)*
                    _ => Err(my_http_utils::http_input::HttpParseError::CanNotParseValue {
                        name: __value.get_name().to_string(),
                        src: __value.get_src(),
                        value: __s,
                    }),
                }
            }
        }
    })
}

fn generate_enum_as_str(cases: &[EnumJson]) -> Result<Vec<proc_macro2::TokenStream>, syn::Error> {
    let mut result = Vec::with_capacity(cases.len());
    for case in cases {
        let case_ident = case.src.get_name_ident();
        let str_value = case.get_enum_case_str_value()?;

        result.push(quote::quote! {
            Self::#case_ident => #str_value,
        });
    }

    Ok(result)
}

fn generate_enum_cases(cases: &[EnumJson]) -> Result<Vec<proc_macro2::TokenStream>, syn::Error> {
    let mut result = Vec::with_capacity(cases.len());
    for case in cases {
        let id = proc_macro2::Literal::isize_unsuffixed(case.get_id()?);
        let value = case.get_enum_case_value();
        let description = case.description();

        result.push(quote::quote! {
            __es.cases.push(data_types::HttpEnumCase{
                id: #id,
                value: #value,
                description: #description
            });
        });
    }

    Ok(result)
}
