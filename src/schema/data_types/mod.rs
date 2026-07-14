mod array_element;
mod data_type;
mod data_type_provider;
mod http_enum_struct;
mod http_field;
mod http_object_type;
mod http_simple_type;

pub use array_element::ArrayElement;
pub use data_type::HttpDataType;

pub use data_type_provider::*;
pub use http_enum_struct::{EnumType, HttpEnumCase, HttpEnumStructure};
pub use http_field::HttpField;
pub use http_object_type::*;
pub use http_simple_type::HttpSimpleType;

pub trait InputStructure {
    fn get_struct_id(&self) -> String;
}
