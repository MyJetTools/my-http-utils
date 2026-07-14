use types_reader::macros::*;

#[attribute_name("http_body_raw")]
#[derive(MacrosParameters, Clone)]
pub struct HttpBodyRawAttribute<'s> {
    pub name: Option<&'s str>,
    pub description: &'s str,

    #[allow_ident]
    pub validator: Option<&'s str>,

    #[has_attribute]
    pub to_lowercase: bool,

    #[has_attribute]
    pub to_uppercase: bool,

    #[has_attribute]
    pub trim: bool,

    pub default: Option<types_reader::AnyValue<'s>>,

    #[has_attribute]
    pub print_request_to_console: bool,
}
