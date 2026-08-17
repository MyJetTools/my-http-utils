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

    // serde, over the SAME string `as_str()` emits. Unconditional: an enum nested in an object
    // structure is written by `JsonValueWriter` (this string) but read back by serde, and an enum
    // inside a `RawDataTyped<T>` payload is serde on both sides — so the two must not disagree in
    // any build.
    let serde_impls = generate_serde_impls(struct_name, &fields)?;

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

        // value -> JSON string. Always emitted so the client request builder can serialise an
        // enum-typed field (top-level or nested in an object) with `my_json`, no serde.
        impl my_http_utils::my_json::json_writer::JsonValueWriter for #struct_name {
            const IS_ARRAY: bool = false;
            fn write(&self, __dest: &mut String) {
                my_http_utils::my_json::json_writer::JsonValueWriter::write(&self.as_str(), __dest);
            }
        }

        #default_trait

        #data_type_provider

        #try_from_input

        #serde_impls
    };

    Ok(result.into())
}

/// Emits `Serialize` / `Deserialize` over the enum's `http_enum_case` value — the same string
/// `as_str()` and `JsonValueWriter` emit, and the same one the schema lists under `enum:`.
///
/// Why the derive owns serde instead of leaving it to the user: an enum nested inside an object
/// structure is *written* by `JsonValueWriter` (the `value` string) and *read back* by serde. A
/// user's `#[derive(Deserialize)]` keys off the Rust **variant name** instead, so the two disagree
/// and the object fails to parse at runtime — `unknown variant \`bright-red\`, expected
/// \`BrightRed\``. Owning both halves here is what keeps client, server, `TryFrom<HttpInputValue>`
/// and Swagger on one string.
///
/// Deriving serde on such an enum is now a `conflicting implementations` error: that is deliberate,
/// and the fix is to drop the `Serialize`/`Deserialize` from the derive list.
///
/// `Deserialize` accepts the `value` **or** the numeric `id`, matching `TryFrom<HttpInputValue>`
/// (which takes both) — one behaviour, not two. It reads through `deserialize_any` so an id that
/// arrives as a JSON number (`5`) is accepted alongside the string form (`"5"`).
fn generate_serde_impls(
    struct_name: &syn::Ident,
    cases: &[EnumJson],
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let struct_name_as_str = struct_name.to_string();

    let mut arms = Vec::with_capacity(cases.len());
    let mut expected = Vec::with_capacity(cases.len());

    for case in cases {
        let variant = case.src.get_name_ident();
        let str_value = case.get_enum_case_str_value()?;

        let mut patterns = vec![str_value.clone()];
        if let Some(id) = case.attr.id.as_ref() {
            if id != &str_value {
                patterns.push(id.clone());
            }
        }
        expected.push(format!("`{}`", str_value));

        let patterns = patterns.iter().map(|p| quote::quote!(#p));
        arms.push(quote::quote! {
            #(#patterns)|* => Ok(#struct_name::#variant),
        });
    }

    let expecting = format!("one of {} (or a case id)", expected.join(", "));

    Ok(quote::quote! {
        // my-json's reader: what an enum nested in an object structure is read through, since that
        // object is now read by `JsonValueReader` rather than serde. Same `as_str()` string the
        // writer emits, and the same value-or-id set `TryFrom<HttpInputValue>` accepts.
        impl<'s> my_http_utils::my_json::json_reader::JsonValueReader<'s> for #struct_name {
            fn from_json_value(
                __value: &my_http_utils::my_json::json_reader::JsonValueRef<'s>,
            ) -> Result<Self, my_http_utils::my_json::json_reader::JsonParseError> {
                // A case id may arrive as a bare JSON number rather than a string.
                // `as_str()` resolves escapes; `as_unescaped_str()` would only strip the quotes,
                // so a variant value the writer had to escape (`"`, `\`, a control char) would
                // come back as `\"`/`\\` and never match its own arm - the client's own round
                // trip would fail with `unknown value`. `None` here means JSON `null` or
                // non-UTF-8 bytes, which keeps the lossy fallback meaningful.
                let __owned = match __value.as_str() {
                    Some(__s) => __s,
                    None => String::from_utf8_lossy(__value.as_slice()).into_owned().into(),
                };

                match __owned.as_str() {
                    #(#arms)*
                    __other => Err(my_http_utils::my_json::json_reader::JsonParseError::new(
                        format!(
                            "unknown value `{}` for {}, expected {}",
                            __other, #struct_name_as_str, #expecting
                        ),
                    )),
                }
            }
        }

        impl my_http_utils::serde::Serialize for #struct_name {
            fn serialize<__S: my_http_utils::serde::Serializer>(
                &self,
                __serializer: __S,
            ) -> Result<__S::Ok, __S::Error> {
                __serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> my_http_utils::serde::Deserialize<'de> for #struct_name {
            fn deserialize<__D: my_http_utils::serde::Deserializer<'de>>(
                __deserializer: __D,
            ) -> Result<Self, __D::Error> {
                struct __CaseVisitor;

                impl<'de> my_http_utils::serde::de::Visitor<'de> for __CaseVisitor {
                    type Value = #struct_name;

                    fn expecting(&self, __f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        __f.write_str(#expecting)
                    }

                    fn visit_str<__E: my_http_utils::serde::de::Error>(
                        self,
                        __v: &str,
                    ) -> Result<Self::Value, __E> {
                        match __v {
                            #(#arms)*
                            __other => Err(__E::custom(format!(
                                "unknown value `{}` for {}, expected {}",
                                __other, #struct_name_as_str, #expecting
                            ))),
                        }
                    }

                    // A numeric case id (`5`), not just its string form (`"5"`).
                    fn visit_i64<__E: my_http_utils::serde::de::Error>(
                        self,
                        __v: i64,
                    ) -> Result<Self::Value, __E> {
                        self.visit_str(__v.to_string().as_str())
                    }

                    fn visit_u64<__E: my_http_utils::serde::de::Error>(
                        self,
                        __v: u64,
                    ) -> Result<Self::Value, __E> {
                        self.visit_str(__v.to_string().as_str())
                    }
                }

                __deserializer.deserialize_any(__CaseVisitor)
            }
        }
    })
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
