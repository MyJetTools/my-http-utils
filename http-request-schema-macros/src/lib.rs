extern crate proc_macro;
use proc_macro::TokenStream;

// This crate emits ONLY abstract, model-describing code — the schema (`DataTypeProvider`,
// input params) and the client request builder (`THttpRequestBuilder`). It has no notion of
// "server": server-only macros (e.g. `http_route`) and request parsing live in my-http-server.
mod attributes;
mod consts;
mod enum_doc;
mod generic_utils;
mod http_input_field;
mod http_input_object_structure;
mod http_object_structure;
mod input_models;
mod json_value_reader_gen;
mod json_value_writer_gen;
mod property_type_ext;
mod field_key;
mod types;

#[proc_macro_derive(
    MyHttpInput,
    attributes(
        http_query,
        http_header,
        http_path,
        http_form_data,
        http_body,
        http_body_raw,
        debug,
    )
)]
pub fn my_http_input_doc_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    let mut debug = false;
    let result = match crate::input_models::generate(&ast, &mut debug) {
        Ok(result) => result,
        Err(err) => err.to_compile_error().into(),
    };

    if debug {
        println!("{}", result);
    }

    result
}

// `attributes(debug)`: generate() honours a `#[debug]` field attribute, so the derive has to
// register it as a helper — without this rustc rejects `#[debug]` before the derive is reached.
//
// `attributes(serde)`: the key names come from `#[serde(rename_all)]` / `#[serde(rename)]`, and an
// inert attribute is only accepted if some derive registers it. Since this derive no longer needs
// serde to be derived at all, nothing else would register it — `#[serde(..)]` on a model that does
// not derive `Serialize`/`Deserialize` would be rejected as `cannot find attribute \`serde\``.
// Registering it here is harmless when serde IS derived: helper attributes are inert, and both
// derives simply read the same tokens.
//
// `attributes(json_name)`: this crate's own `#[json_name("cardNumber")]`, which says the same thing
// without making a serde-free model derive serde just to register the attribute.
#[proc_macro_derive(MyHttpInputObjectStructure, attributes(debug, serde, json_name))]
pub fn my_http_input_object_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    let (result, debug) = crate::http_input_object_structure::generate(&ast);

    if debug {
        println!("{}", result);
    }

    result
}

#[proc_macro_derive(MyHttpObjectStructure, attributes(debug, serde, json_name))]
pub fn my_http_output_object_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    let mut debug = false;
    let result = match crate::http_object_structure::generate(&ast, &mut debug) {
        Ok(result) => result,
        Err(err) => err.to_compile_error().into(),
    };

    if debug {
        println!("{}", result);
    }
    result
}

#[proc_macro_derive(MyHttpStringEnum, attributes(http_enum_case))]
pub fn my_http_string_enum_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    match crate::enum_doc::generate(&ast, false) {
        Ok(result) => result,
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_derive(MyHttpIntegerEnum, attributes(http_enum_case))]
pub fn my_http_integer_enum_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    match crate::enum_doc::generate(&ast, true) {
        Ok(result) => result,
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_attribute]
pub fn http_input_field(input: TokenStream, item: TokenStream) -> TokenStream {
    match crate::http_input_field::generate(input, item) {
        Ok(result) => result,
        Err(err) => err.to_compile_error().into(),
    }
}
