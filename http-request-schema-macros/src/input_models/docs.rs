use proc_macro2::TokenStream;

use quote::quote;

use super::{http_input_props::HttpInputProperties, InputField};

pub fn generate_http_input<'s>(
    input_fields: &'s HttpInputProperties<'s>,
) -> Result<TokenStream, syn::Error> {
    let mut doc_fields = Vec::new();
    for struct_property in input_fields.get_all() {
        match generate_http_input_parameter(struct_property) {
            Ok(field) => doc_fields.push(field),
            Err(e) => doc_fields.push(e.to_compile_error()),
        }
    }

    let use_documentation = crate::consts::get_use_documentation();
    let result = quote! {
        #use_documentation;
        vec![#(#doc_fields),*]
    };

    Ok(result)
}

fn generate_http_input_parameter(input_field: &InputField) -> Result<TokenStream, syn::Error> {
    let field = crate::types::compile_http_field(
        input_field.get_input_field_name()?,
        &input_field.property.ty,
        input_field.attr.has_default(),
    )?;

    let http_input_parameter_type = crate::consts::get_http_input_parameter();
    let description = input_field.get_description();

    let source = input_field.attr.to_src_token_stream();

    let result = quote! {
        #http_input_parameter_type{
            field: #field,
            description: #description.to_string(),
            source: #source
        }
    };

    Ok(result)
}
