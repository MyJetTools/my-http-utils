//! Codegen for reading an object structure back out of a JSON body with `my_json`'s
//! `JsonValueReader` — the mirror of [`crate::json_value_writer_gen`], and deliberately so.
//!
//! The two halves of a nested object's wire contract are generated from the **same** field
//! metadata: the same `get_name()` key (so `#[serde(rename_all)]` cannot apply to one side only),
//! and the same per-type handling. Previously the object was written by `my-json` here and read
//! back by serde, which is what let the two drift apart — `rename_all` silently ignored, a nested
//! `DateTimeAsMicroseconds` written as RFC-3339 but read as an `i64`, a nested enum written as its
//! `http_enum_case` value but read as its Rust variant name. One contract, one source of truth.

use proc_macro2::TokenStream;
use quote::quote;
use types_reader::StructProperty;

use crate::http_object_structure::struct_prop_ext::StructPropertyExt;
use crate::field_key::RenameAllRule;

/// Emits `impl JsonValueReader<'s> for #struct_name`.
///
/// Generic object structures emit nothing, matching
/// [`crate::json_value_writer_gen::generate_object_json_value_writer`]: that one bails on generics,
/// so a generic object cannot be written into a request by the client — being able to read one the
/// client cannot send would just move the asymmetry rather than close it.
pub fn generate_object_json_value_reader(
    struct_name: &syn::Ident,
    is_generic: bool,
    fields: &[StructProperty],
    rename_all: Option<RenameAllRule>,
) -> Result<TokenStream, syn::Error> {
    if is_generic {
        return Ok(quote!());
    }

    let mut reads = Vec::with_capacity(fields.len());
    for field in fields {
        let key = field.get_name(rename_all)?;
        let ident = field.get_field_name_ident();

        // The key is resolved exactly as the writer resolves it — one `get_name`, two halves.
        reads.push(quote! {
            #ident: my_http_utils::read_json_object_field(__raw, #key)?
        });
    }

    Ok(quote! {
        impl<'s> my_http_utils::my_json::json_reader::JsonValueReader<'s> for #struct_name {
            fn from_json_value(
                __value: &my_http_utils::my_json::json_reader::JsonValueRef<'s>,
            ) -> Result<Self, my_http_utils::my_json::json_reader::JsonParseError> {
                // The member's verbatim source slice, borrowed for the whole `'s` — that is what
                // lets a nested object recurse into this same impl.
                let __raw = __value.as_slice();
                Ok(Self { #(#reads),* })
            }
        }
    })
}
