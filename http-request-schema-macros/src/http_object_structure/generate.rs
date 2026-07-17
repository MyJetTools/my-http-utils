use quote::quote;
use types_reader::StructProperty;

use crate::generic_utils::GenericData;

pub fn generate(
    ast: &syn::DeriveInput,
    debug: &mut bool,
) -> Result<proc_macro::TokenStream, syn::Error> {
    let struct_name = &ast.ident;

    let fields = StructProperty::read(ast)?;

    for prop in &fields {
        if prop.attrs.has_attr("debug") {
            *debug = true;
        }
    }

    let generic_data = GenericData::new(ast);

    // The container's `#[serde(rename_all = "..")]`, read once and used for both the written keys
    // and the schema's field names, so all three agree with serde.
    let rename_all = crate::field_key::read_container_rename_all(ast)?;

    // `JsonValueWriter` — always emitted so the client request builder can serialise this object
    // (whether it is the whole body or nested) with `my_json`, no serde.
    let json_value_writer = crate::json_value_writer_gen::generate_object_json_value_writer(
        struct_name,
        generic_data.is_some(),
        &fields,
        rename_all,
    )?;

    // OpenAPI/Swagger schema — server concern; emit nothing for client (default) builds.
    let data_structure_provider = if cfg!(feature = "server") {
        let get_http_data_structure = super::generate_get_http_data_structure(
            struct_name,
            generic_data.as_ref(),
            &fields,
            rename_all,
        )?;
        crate::http_object_structure::generate_data_provider(
            struct_name,
            generic_data.as_ref(),
            get_http_data_structure,
        )?
    } else {
        quote!()
    };

    let result = quote! {
        #json_value_writer
        #data_structure_provider
    }
    .into();

    Ok(result)
}
