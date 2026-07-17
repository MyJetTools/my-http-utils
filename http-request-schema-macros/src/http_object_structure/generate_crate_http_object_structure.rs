use types_reader::StructProperty;

use crate::generic_utils::GenericData;
use crate::field_key::RenameAllRule;

use super::struct_prop_ext::StructPropertyExt;

pub fn generate_get_http_data_structure(
    struct_name: &syn::Ident,
    generic_data: Option<&GenericData>,
    fields: &[StructProperty],
    rename_all: Option<RenameAllRule>,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let use_documentation = crate::consts::get_use_documentation();

    let struct_name_as_str = struct_name.to_string();

    // The same key the client writes and the server reads — so Swagger documents the real
    // wire name rather than the Rust field name.
    let fields = render_obj_fields(fields, rename_all)?;

    let generic_name = if let Some(generic) = generic_data {
        let ident = &generic.generic_ident;
        quote::quote!(#ident::get_generic_type())
    } else {
        quote::quote!(None)
    };

    let result = quote::quote! {
        fn get_http_data_structure()->my_http_utils::schema::data_types::HttpObjectStructure{
            #use_documentation;

            let mut __hos = data_types::HttpObjectStructure::new(#struct_name_as_str, #generic_name);
            #(#fields)*
            __hos
        }
    };

    Ok(result)
}

fn render_obj_fields(
    fields: &[StructProperty],
    rename_all: Option<RenameAllRule>,
) -> Result<Vec<proc_macro2::TokenStream>, syn::Error> {
    let mut result = Vec::with_capacity(fields.len());
    for field in fields {
        let name = field.get_name(rename_all)?;
        let line =
            crate::types::compile_http_field(name.as_str(), &field.ty, field.ty.is_option())?;

        result.push(quote::quote!(__hos.main.fields.push(#line);));
    }

    Ok(result)
}
