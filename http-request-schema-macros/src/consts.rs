use proc_macro2::TokenStream;
use quote::quote;

pub fn get_http_field_type() -> TokenStream {
    quote!(data_types::HttpField)
}

// Brings the schema modules (and the `data_types` glob) into scope so the short
// `data_types::…`, `in_parameters::…`, `out_results::…` paths emitted throughout the
// generators resolve to `my_http_utils::schema`.
pub fn get_use_documentation() -> TokenStream {
    quote!(
        use my_http_utils::schema::{data_types, in_parameters, out_results, data_types::*};
    )
}

pub fn get_http_input_parameter() -> TokenStream {
    quote!(in_parameters::HttpInputParameter)
}

pub fn get_http_input_parameter_with_ns() -> TokenStream {
    quote!(my_http_utils::schema::in_parameters::HttpInputParameter)
}

pub fn get_http_parameter_input_src() -> TokenStream {
    quote!(in_parameters::HttpParameterInputSource)
}
