use proc_macro2::TokenStream;
use quote::quote;
use types_reader::PropertyType;

use crate::property_type_ext::PropertyTypeExt;

/// Emits one `HttpField::new(..)` for a model field.
///
/// `wire_name` is the field's key on the wire — the schema's own name for it. `struct_id` and
/// `rust_field_name` are *not* part of the schema: they are handed to the field's
/// `DataTypeProvider` so that a type with no schema representation panics with `Struct::field`
/// instead of with just the element type it choked on. The schema is built when a controller is
/// registered, i.e. on start-up, so that message is all the operator gets before the process dies;
/// see `SchemaFieldCtx` in my-http-utils. The **Rust** name is the one carried there, because the
/// answer the message has to support is "which line do I open", and a `#[serde(rename)]`d field
/// cannot be found in the source by its wire name.
pub fn compile_http_field(
    struct_id: &str,
    wire_name: &str,
    rust_field_name: &str,
    pt: &PropertyType,
    has_defaults_value: bool,
) -> Result<TokenStream, syn::Error> {
    let data_type = compile_data_type(struct_id, rust_field_name, pt);
    let mut required = pt.required();

    if has_defaults_value {
        required = false;
    }

    let http_field_type = crate::consts::get_http_field_type();

    let result = quote! {
        #http_field_type::new(#wire_name, #data_type, #required)
    };

    Ok(result)
}

fn compile_data_type(struct_id: &str, field_name: &str, pt: &PropertyType) -> TokenStream {
    // An `Option<T>` is described as `T` — optionality is carried by `HttpField::required`, not by
    // the data type.
    let type_token = match pt {
        PropertyType::OptionOf(generic_type) => generic_type.get_token_stream_with_generics(),
        _ => pt.get_token_stream_with_generics(),
    };

    let data_type_provider = crate::consts::get_data_type_provider_with_ns();
    let schema_field_ctx = crate::consts::get_schema_field_ctx_with_ns();

    // Fully qualified: the call site is generated code in the consumer's crate, so neither the
    // trait being in scope nor the absence of a same-named inherent method can be assumed.
    quote!(
        <#type_token as #data_type_provider>::get_data_type_in_field(
            #schema_field_ctx::new(#struct_id, #field_name)
        )
    )
}
