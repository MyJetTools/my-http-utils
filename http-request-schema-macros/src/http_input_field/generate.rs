// The `MacrosEnum` derive (types-reader) expands to code clippy flags as
// `needless_question_mark`; it's third-party generated code, so silence it for this file.
#![allow(clippy::needless_question_mark)]

use std::str::FromStr;

use proc_macro::TokenStream;
use types_reader::macros::MacrosEnum;
use types_reader::TokensObject;

#[derive(MacrosEnum)]
pub enum OpenApiType {
    #[default]
    String,
    Password,
}

impl OpenApiType {
    pub fn get_macros_stream(&self) -> proc_macro2::TokenStream {
        match self {
            Self::String => proc_macro2::TokenStream::from_str("String").unwrap(),
            Self::Password => proc_macro2::TokenStream::from_str("Password").unwrap(),
        }
    }
}

pub fn generate(attr: TokenStream, input: TokenStream) -> Result<TokenStream, syn::Error> {
    let input: proc_macro2::TokenStream = input.into();
    let src = quote::quote!(#input);

    let tuple_struct = types_reader::SingleValueTupleStruct::new(input)?;

    let attr: proc_macro2::TokenStream = attr.into();

    // Accept the short `tp` alias, the legacy `open_api_type` parameter, or a single positional
    // value (e.g. `#[http_input_field("Password")]`). An empty attribute defaults to String.
    let open_api_type: OpenApiType = if attr.is_empty() {
        OpenApiType::default()
    } else {
        let attr = TokensObject::new(attr.into())?;
        if let Some(param) = attr.try_get_named_param("open_api_type") {
            param.unwrap_as_value()?.try_into()?
        } else {
            attr.get_value_from_single_or_named("tp")?.try_into()?
        }
    };

    let simple_type = open_api_type.get_macros_stream();

    let struct_name = &tuple_struct.name_ident;

    let tp = &tuple_struct.type_ident;

    // Server-side conversion from a parsed request value — reads the value as a String and wraps
    // it. Only emitted with the `server` feature (it references the server-gated `http_input`).
    let try_from_input = if cfg!(feature = "server") {
        quote::quote! {
            impl<'s> std::convert::TryFrom<my_http_utils::http_input::HttpInputValue<'s>> for #struct_name {
                type Error = my_http_utils::http_input::HttpParseError;
                fn try_from(
                    __value: my_http_utils::http_input::HttpInputValue<'s>,
                ) -> Result<Self, Self::Error> {
                    Ok(Self::new(__value.as_string()?))
                }
            }
        }
    } else {
        quote::quote!()
    };

    // Schema description of the field's type — OpenAPI/Swagger, server only.
    let data_type_provider = if cfg!(feature = "server") {
        quote::quote! {
            impl my_http_utils::schema::data_types::DataTypeProvider for #struct_name {
                fn get_data_type() -> my_http_utils::schema::data_types::HttpDataType {
                    my_http_utils::schema::data_types::HttpDataType::SimpleType(
                        my_http_utils::schema::data_types::HttpSimpleType::#simple_type,
                    )
                }
            }
        }
    } else {
        quote::quote!()
    };

    let result = quote::quote! {
        #[derive(Debug)]
        #src

        #data_type_provider

            impl #struct_name {
                pub fn new(value: impl Into<#tp>) -> Self {
                    Self(value.into())
                }

                // The client request builder relies on `as_str()` to serialize the field.
                pub fn as_str(&self) -> &str {
                    self.0.as_str()
                }
            }

            impl Into<#tp> for #struct_name {
                fn into(self) -> #tp {
                    self.0
                }
            }

            #try_from_input
    };

    Ok(result.into())
}
