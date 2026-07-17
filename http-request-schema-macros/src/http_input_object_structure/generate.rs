use types_reader::StructProperty;

use crate::generic_utils::GenericData;

/// `MyHttpInputObjectStructure` — a nested **input** object: both halves of its wire contract.
///
/// * always — the `JsonValueWriter` the client request builder uses to write the object into a
///   request body;
/// * behind `server` — the schema (`DataTypeProvider`) and the matching read half,
///   `TryFrom<HttpInputValue>`, which is what lets the object be a `#[http_body]` field of a
///   `MyHttpInput` model. The parse layer used to live in my-http-server, which is why this read
///   half was once out of scope; it now lives here, so both halves are emitted together.
pub fn generate(ast: &syn::DeriveInput) -> (proc_macro::TokenStream, bool) {
    let struct_name = &ast.ident;

    let mut debug = false;

    let fields = match StructProperty::read(ast) {
        Ok(result) => result,
        Err(err) => return (err.into_compile_error().into(), debug),
    };

    for field in &fields {
        if field.attrs.has_attr("debug") {
            debug = true;
        }
    }

    let generic_data = GenericData::new(ast);

    // The container's `#[serde(rename_all = "..")]`. This object is written into the body by the
    // `JsonValueWriter` below and read back out of it by serde (via the `TryFrom` at the bottom),
    // so the keys must be the ones serde names.
    let rename_all = match crate::field_key::read_container_rename_all(ast) {
        Ok(result) => result,
        Err(err) => return (err.into_compile_error().into(), debug),
    };

    // `JsonValueWriter` — always emitted so the client request builder can serialise this object
    // (whether it is the whole body or nested) with `my_json`, no serde.
    let json_value_writer = match crate::json_value_writer_gen::generate_object_json_value_writer(
        struct_name,
        generic_data.is_some(),
        &fields,
        rename_all,
    ) {
        Ok(result) => result,
        Err(err) => return (err.into_compile_error().into(), debug),
    };

    // Schema-only description (a nested input object's `DataTypeProvider`) — server concern; emit
    // nothing for client (default) builds.
    let data_structure_provider = if cfg!(feature = "server") {
        let get_http_data_structure =
            match crate::http_object_structure::generate_get_http_data_structure(
                struct_name,
                generic_data.as_ref(),
                &fields,
                rename_all,
            ) {
                Ok(result) => result,
                Err(err) => return (err.into_compile_error().into(), debug),
            };

        match crate::http_object_structure::generate_data_provider(
            struct_name,
            generic_data.as_ref(),
            get_http_data_structure,
        ) {
            Ok(result) => result,
            Err(err) => return (err.into_compile_error().into(), debug),
        }
    } else {
        quote::quote!()
    };

    // `JsonValueReader` — the read half of the very contract `JsonValueWriter` above writes, built
    // from the same fields and the same keys. Always emitted, like the writer: the two are a pair,
    // and my-json (which owns the trait) is not itself gated.
    let json_value_reader = match crate::json_value_reader_gen::generate_object_json_value_reader(
        struct_name,
        generic_data.is_some(),
        &fields,
        rename_all,
    ) {
        Ok(result) => result,
        Err(err) => return (err.into_compile_error().into(), debug),
    };

    // Request value -> object. Server-gated, like the `MyHttpStringEnum` / `http_input_field`
    // equivalents: it names `my_http_utils::http_input::HttpInputValue`, which is itself behind the
    // `server` feature.
    //
    // Emitted only for non-generic structs, to stay in step with `JsonValueWriter` — that one bails
    // on generics, so a generic object structure cannot be written into a request by the client.
    // Reading one the client can't send would just move the client/server asymmetry rather than
    // close it.
    //
    // It reads through `JsonValueReader` (above), NOT serde: that is what keeps the two halves from
    // drifting, and it is why this derive does not require `Deserialize` at all.
    let try_from_input = if cfg!(feature = "server") && generic_data.is_none() {
        quote::quote! {
            impl<'s> std::convert::TryFrom<my_http_utils::http_input::HttpInputValue<'s>> for #struct_name {
                type Error = my_http_utils::http_input::HttpParseError;
                fn try_from(
                    __value: my_http_utils::http_input::HttpInputValue<'s>,
                ) -> Result<Self, Self::Error> {
                    __value.read_json_object()
                }
            }
        }
    } else {
        quote::quote!()
    };

    let result = quote::quote! {
        #json_value_writer
        #json_value_reader
        #data_structure_provider
        #try_from_input
    };

    (result.into(), debug)
}
