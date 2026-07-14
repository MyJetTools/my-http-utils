pub fn generate_default_as_str_fn(default_case: Option<&String>) -> proc_macro2::TokenStream {
    match default_case {
        Some(value) => quote::quote! {
                pub fn default_as_str() -> &'static str {
                    #value
                }
        },
        None => quote::quote!(),
    }
}
