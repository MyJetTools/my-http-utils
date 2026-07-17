//! Shared codegen for building a JSON object with `my_json`'s `JsonObjectWriter`. Used both by the
//! client `#[http_body]` request builder ([`crate::input_models::client_writer`]) and by the
//! `JsonValueWriter` impls emitted for object structures — so a struct serialises identically
//! whether it is the whole body or nested inside another object.

use proc_macro2::TokenStream;
use quote::quote;
use types_reader::{PropertyType, StructProperty};

use crate::http_object_structure::struct_prop_ext::StructPropertyExt;
use crate::field_key::RenameAllRule;

/// Emits the `__obj.write_*(key, …)` expression appending one field into `__obj` (a
/// `JsonObjectWriter`) and returning the updated writer. `place` is the field access expression
/// (`self.ident`); it is only ever borrowed (`&place`) or copied, so the same expression is valid
/// whether `self` is owned (client builder) or borrowed (`JsonValueWriter::write(&self, …)`).
///
/// * `Option` → `write_if_some_ref` (a `None` omits the key);
/// * `Vec` → an array (`Vec<T>` is itself a `JsonValueWriter`);
/// * everything else (scalars, `String`, `DateTimeAsMicroseconds`, and any `Struct` that
///   implements `JsonValueWriter` — object structures, enums, custom fields) → `write_ref`.
///
/// `DateTimeAsMicroseconds` deliberately has **no** special case here. `my-json` implements
/// `JsonValueWriter` for it (it depends on `rust-extensions`), so it goes the common path and the
/// date's spelling is decided in exactly one place. Formatting it here instead used to call
/// `to_rfc3339()` directly, which silently bypassed that impl and emitted `…+00:00` while
/// `rust-extensions`' own serde emitted `…Z` — two spellings of one type on the wire.
pub fn json_object_field_write(key: &str, place: &TokenStream, ty: &PropertyType) -> TokenStream {
    match ty {
        PropertyType::OptionOf(_) => quote!(__obj.write_if_some_ref(#key, &#place)),
        PropertyType::VecOf(_) => quote!(__obj.write_ref(#key, &#place)),
        _ => quote!(__obj.write_ref(#key, &#place)),
    }
}

/// Emits `impl JsonValueWriter for #struct_name`, serialising the struct as a JSON object whose
/// keys are the fields' `#[serde(rename)]`/Rust names. Emitted unconditionally (the client needs it
/// to serialise a nested object), so it does not reference the server-gated schema layer.
///
/// Generic object structures keep the previous serde path on the client — an accurate
/// `impl<T: JsonValueWriter> …` bound is not reconstructable from the schema's `GenericData`, so we
/// emit nothing rather than an unsound impl.
///
/// `rename_all` is the container's `#[serde(rename_all = "..")]`. The keys written here have to be
/// the ones serde would look for: the server reads an object structure back out of the body with
/// serde, so a key this writer invents on its own would simply not be found.
pub fn generate_object_json_value_writer(
    struct_name: &syn::Ident,
    is_generic: bool,
    fields: &[StructProperty],
    rename_all: Option<RenameAllRule>,
) -> Result<TokenStream, syn::Error> {
    if is_generic {
        return Ok(quote!());
    }

    let mut writes = Vec::with_capacity(fields.len());
    for field in fields {
        let key = field.get_name(rename_all)?;
        let ident = field.get_field_name_ident();
        let place = quote!(self.#ident);
        writes.push(json_object_field_write(key.as_str(), &place, &field.ty));
    }

    Ok(quote! {
        impl my_http_utils::my_json::json_writer::JsonValueWriter for #struct_name {
            const IS_ARRAY: bool = false;
            fn write(&self, __dest: &mut String) {
                let __obj = my_http_utils::my_json::json_writer::JsonObjectWriter::new();
                #(let __obj = #writes;)*
                __obj.build_into(__dest);
            }
        }
    })
}
