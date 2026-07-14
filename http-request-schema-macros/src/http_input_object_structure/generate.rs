use types_reader::StructProperty;

use crate::generic_utils::GenericData;

/// `MyHttpInputObjectStructure` — schema description only (a nested input object's
/// `DataTypeProvider`). Reading such an object out of a request is a server concern and lives
/// in my-http-server; here we only describe the model.
pub fn generate(ast: &syn::DeriveInput) -> (proc_macro::TokenStream, bool) {
    // Schema-only derive (a nested input object's `DataTypeProvider`) — server concern. Emit
    // nothing for client (default) builds.
    if !cfg!(feature = "server") {
        return (proc_macro::TokenStream::new(), false);
    }

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

    let get_http_data_structure =
        match crate::http_object_structure::generate_get_http_data_structure(
            struct_name,
            generic_data.as_ref(),
            &fields,
        ) {
            Ok(result) => result,
            Err(err) => return (err.into_compile_error().into(), debug),
        };

    let data_structure_provider = match crate::http_object_structure::generate_data_provider(
        struct_name,
        generic_data.as_ref(),
        get_http_data_structure,
    ) {
        Ok(result) => result,
        Err(err) => return (err.into_compile_error().into(), debug),
    };

    (data_structure_provider.into(), debug)
}
