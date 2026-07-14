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

        // Schema description of the enum.
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
    };

    Ok(result.into())
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
