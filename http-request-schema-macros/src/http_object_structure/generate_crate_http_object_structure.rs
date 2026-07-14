use types_reader::StructProperty;

use crate::generic_utils::GenericData;

use super::struct_prop_ext::StructPropertyExt;

pub fn generate_get_http_data_structure(
    struct_name: &syn::Ident,
    generic_data: Option<&GenericData>,
    fields: &[StructProperty],
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let use_documentation = crate::consts::get_use_documentation();

    let struct_name_as_str = struct_name.to_string();

    let fields = render_obj_fields(fields)?;

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
) -> Result<Vec<proc_macro2::TokenStream>, syn::Error> {
    let mut result = Vec::with_capacity(fields.len());
    for field in fields {
        let line =
            crate::types::compile_http_field(field.get_name()?, &field.ty, field.ty.is_option())?;

        result.push(quote::quote!(__hos.main.fields.push(#line);));
    }

    Ok(result)
}
