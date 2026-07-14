//! Per-field codegen helpers used only by the server-side `parse` generator (behind the
//! `server` feature). Ported from `my-http-server-macros::input_models::input_field`, adapted to
//! the my-http-utils attribute API (`has_trim`/`has_to_lowercase`/…) and the new validator
//! contract `fn(&FieldValue) -> Result<(), HttpParseError>` (no `ctx`).

use std::str::FromStr;

use proc_macro2::TokenStream;
use quote::quote;
use types_reader::PropertyType;

use super::InputField;

impl<'s> InputField<'s> {
    fn is_str(&self) -> bool {
        match &self.property.ty {
            PropertyType::RefTo { ty, .. } => ty.as_str().as_str() == "str",
            PropertyType::String => true,
            PropertyType::OptionOf(sub_ty) => match sub_ty.as_ref() {
                PropertyType::RefTo { ty, .. } => ty.as_str().as_str() == "str",
                PropertyType::String => true,
                _ => false,
            },
            _ => false,
        }
    }

    pub fn has_default_value(&self) -> bool {
        self.attr.has_default()
    }

    fn to_lower_case_string(&self) -> Result<bool, syn::Error> {
        let result = self.attr.has_to_lowercase();
        if result && !self.is_str() {
            return self
                .throw_error("to_lowercase attribute can be used only with a String property");
        }
        Ok(result)
    }

    fn to_upper_case_string(&self) -> Result<bool, syn::Error> {
        let result = self.attr.has_to_uppercase();
        if result && !self.is_str() {
            return self
                .throw_error("to_uppercase attribute can be used only with a String property");
        }
        Ok(result)
    }

    /// The struct-literal entry for this field: the raw local, with `trim` / `to_lowercase` /
    /// `to_uppercase` applied (String / `&str` only, after reading).
    pub fn read_value_with_transformation(&self) -> Result<TokenStream, syn::Error> {
        let ident = self.property.get_field_name_ident();

        let trim = if self.attr.has_trim() {
            quote!(.trim())
        } else {
            quote!()
        };

        if self.to_upper_case_string()? {
            if self.property.ty.is_option() {
                return Ok(quote!(#ident: if let Some(value) = #ident { value #trim .to_uppercase().into() } else { None }));
            }
            return Ok(quote!(#ident: #ident #trim .to_uppercase()));
        }

        if self.to_lower_case_string()? {
            if self.property.ty.is_option() {
                return Ok(quote!(#ident: if let Some(value) = #ident { value #trim .to_lowercase().into() } else { None }));
            }
            return Ok(quote!(#ident: #ident #trim .to_lowercase()));
        }

        if self.attr.has_trim() {
            if self.property.ty.is_option() {
                return Ok(quote!(#ident: if let Some(value) = #ident { value.trim().to_string().into() } else { None }));
            }
            return Ok(quote!(#ident: #ident.trim().to_string()));
        }

        Ok(quote!(#ident))
    }

    /// The `let` binding pattern (with a type annotation for String fields, so transforms and
    /// `.into()` type-check).
    pub fn get_let_input_param(&self) -> TokenStream {
        let struct_name = self.property.get_field_name_ident();
        match &self.property.ty {
            PropertyType::String => quote!(#struct_name: String),
            PropertyType::OptionOf(sub_ty) => match sub_ty.as_ref() {
                PropertyType::String => quote!(#struct_name: Option<String>),
                _ => quote!(#struct_name),
            },
            _ => quote!(#struct_name),
        }
    }

    /// `default` value expression for an `Option<T>` field (the `else` branch of an optional
    /// read): `None`, `Some(literal)`, or `create_default()?`.
    pub fn get_default_value_opt_case(&'s self) -> Result<TokenStream, syn::Error> {
        let default_value = match self.attr.get_default() {
            Some(value) => value,
            None => return Ok(quote!(None)),
        };

        if default_value.has_empty_value() {
            let name = self.property.ty.get_token_stream();
            return Ok(quote!(#name::create_default()?));
        }

        if let PropertyType::OptionOf(pt) = &self.property.ty {
            match pt.as_ref() {
                PropertyType::U8 => {
                    let value = default_value.get_value().unwrap_as_number()?.as_u8();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::I8 => {
                    let value = default_value.get_value().unwrap_as_number()?.as_i8();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::U16 => {
                    let value = default_value.get_value().unwrap_as_number()?.as_u16();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::I16 => {
                    let value = default_value.get_value().unwrap_as_number()?.as_i16();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::U32 => {
                    let value = default_value.get_value().unwrap_as_number()?.as_u32();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::I32 => {
                    let value = default_value.get_value().unwrap_as_number()?.as_i32();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::U64 => {
                    let value = default_value.get_value().unwrap_as_number()?.as_u64();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::I64 => {
                    let value = default_value.get_value().unwrap_as_number()?.as_i64();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::F32 => {
                    let value = default_value.get_value().unwrap_as_double()?.as_f32();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::F64 => {
                    let value = default_value.get_value().unwrap_as_double()?.as_f64();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::USize => {
                    let value = default_value.get_value().unwrap_as_number()?.as_usize();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::ISize => {
                    let value = default_value.get_value().unwrap_as_number()?.as_isize();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::String => {
                    let value = default_value.get_value().unwrap_as_string()?.as_str();
                    return Ok(quote!(Some(#value.to_string())));
                }
                PropertyType::Bool => {
                    let value = default_value.get_value().unwrap_as_bool()?.get_value();
                    return Ok(quote!(Some(#value)));
                }
                PropertyType::DateTime => {
                    let value = default_value.get_value().unwrap_as_string()?.as_str();
                    return Ok(quote!(Some(DateTimeAsMicroseconds::from_str(#value).unwrap())));
                }
                PropertyType::RefTo { ty, .. } => {
                    if ty.as_str().as_str() == "str" {
                        let value = default_value.get_value().unwrap_as_string()?.as_str();
                        return Ok(quote!(Some(#value)));
                    }
                }
                _ => return Ok(quote!(None)),
            }
        }

        Ok(quote!(None))
    }

    /// `default` value expression for a non-`Option` field.
    pub fn get_default_value_non_opt_case(&self) -> Result<TokenStream, syn::Error> {
        let default_value = match self.attr.get_default() {
            Some(value) => value,
            None => return Ok(quote!(None)),
        };

        if default_value.has_empty_value() {
            let name = self.property.ty.get_token_stream();
            return Ok(quote!(#name::create_default()?));
        }

        match &self.property.ty {
            PropertyType::U8 => {
                let value = default_value.get_value().unwrap_as_number()?.as_u8();
                Ok(quote!(#value))
            }
            PropertyType::I8 => {
                let value = default_value.get_value().unwrap_as_number()?.as_i8();
                Ok(quote!(#value))
            }
            PropertyType::U16 => {
                let value = default_value.get_value().unwrap_as_number()?.as_u16();
                Ok(quote!(#value))
            }
            PropertyType::I16 => {
                let value = default_value.get_value().unwrap_as_number()?.as_i16();
                Ok(quote!(#value))
            }
            PropertyType::U32 => {
                let value = default_value.get_value().unwrap_as_number()?.as_u32();
                Ok(quote!(#value))
            }
            PropertyType::I32 => {
                let value = default_value.get_value().unwrap_as_number()?.as_i32();
                Ok(quote!(#value))
            }
            PropertyType::U64 => {
                let value = default_value.get_value().unwrap_as_number()?.as_u64();
                Ok(quote!(#value))
            }
            PropertyType::I64 => {
                let value = default_value.get_value().unwrap_as_number()?.as_i64();
                Ok(quote!(#value))
            }
            PropertyType::F32 => {
                let value = default_value.get_value().unwrap_as_double()?.as_f32();
                Ok(quote!(#value))
            }
            PropertyType::F64 => {
                let value = default_value.get_value().unwrap_as_double()?.as_f64();
                Ok(quote!(#value))
            }
            PropertyType::USize => {
                let value = default_value.get_value().unwrap_as_number()?.as_usize();
                Ok(quote!(#value))
            }
            PropertyType::ISize => {
                let value = default_value.get_value().unwrap_as_number()?.as_isize();
                Ok(quote!(#value))
            }
            PropertyType::String => {
                let value = default_value.get_value().unwrap_as_string()?.as_str();
                Ok(quote!(#value.to_string()))
            }
            PropertyType::Bool => {
                let value = default_value.get_value().unwrap_as_bool()?.get_value();
                Ok(quote!(#value))
            }
            PropertyType::DateTime => {
                let value = default_value.get_value().unwrap_as_string()?.as_str();
                Ok(quote!(DateTimeAsMicroseconds::from_str(#value).unwrap()))
            }
            PropertyType::Struct(..) => {
                let name = self.property.ty.get_token_stream();
                Ok(quote!(#name::create_default()?))
            }
            PropertyType::RefTo { ty, .. } => {
                if ty.as_str().as_str() == "str" {
                    let value = default_value.get_value().unwrap_as_string()?.as_str();
                    return Ok(quote!(#value));
                }
                self.throw_error("Unsupported type for default value")
            }
            PropertyType::OptionOf(_) => self.throw_error("Option default value is not supported"),
            PropertyType::VecOf(_) => self.throw_error("Vec default value is not supported"),
            PropertyType::HashMap(_, _) => {
                self.throw_error("HashMap default value is not supported")
            }
        }
    }

    /// The validator call, if a `validator = "fn"` was given.
    ///
    /// Contract: `fn(&str) -> Result<(), impl ToString>` — the SAME contract the client request
    /// builder uses, so one validator function works on both sides (`ctx` is dropped — put
    /// context-dependent checks in the action). The parse layer applies it to the field's `&str`
    /// form (so validators are for `String` / `Option<String>` / `&str` fields) and maps the
    /// error into `HttpParseError::Validation`. Runs after the field is read, before the final
    /// trim/case transform (matching the old server codegen).
    pub fn get_validator_as_token_stream(&self) -> Option<TokenStream> {
        let validator = self.attr.validator()?;
        let validation_fn = TokenStream::from_str(validator).unwrap();
        let field = self.property.get_field_name_ident();

        let call = if self.property.ty.is_option() {
            quote! {
                if let Some(__v) = #field.as_ref() {
                    if let Err(__e) = #validation_fn(__v.as_ref()) {
                        return Err(my_http_utils::http_input::HttpParseError::Validation(__e.to_string()));
                    }
                }
            }
        } else {
            quote! {
                if let Err(__e) = #validation_fn(#field.as_ref()) {
                    return Err(my_http_utils::http_input::HttpParseError::Validation(__e.to_string()));
                }
            }
        };

        Some(call)
    }
}
