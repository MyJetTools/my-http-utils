use types_reader::macros::*;

#[attribute_name("http_enum_case")]
#[derive(MacrosParameters, Clone)]
pub struct EnumCaseAttribute {
    #[any_value_as_string]
    pub id: Option<String>,

    pub value: Option<String>,

    pub description: String,

    #[has_attribute]
    pub default: bool,
}
