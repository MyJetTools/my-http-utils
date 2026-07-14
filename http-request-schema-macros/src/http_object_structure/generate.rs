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

    let get_http_data_structure =
        super::generate_get_http_data_structure(struct_name, generic_data.as_ref(), &fields)?;

    let data_structure_provider = crate::http_object_structure::generate_data_provider(
        struct_name,
        generic_data.as_ref(),
        get_http_data_structure,
    )?;

    let result = quote! {

        #data_structure_provider

    }
    .into();

    Ok(result)
}
