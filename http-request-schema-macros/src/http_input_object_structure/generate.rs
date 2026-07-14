use types_reader::StructProperty;

use crate::generic_utils::GenericData;

/// `MyHttpInputObjectStructure` — schema description only (a nested input object's
/// `DataTypeProvider`). Reading such an object out of a request is a server concern and lives
/// in my-http-server; here we only describe the model.
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

    // `JsonValueWriter` — always emitted so the client request builder can serialise this object
    // (whether it is the whole body or nested) with `my_json`, no serde.
    let json_value_writer = match crate::json_value_writer_gen::generate_object_json_value_writer(
        struct_name,
        generic_data.is_some(),
        &fields,
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

    let result = quote::quote! {
        #json_value_writer
        #data_structure_provider
    };

    (result.into(), debug)
}
