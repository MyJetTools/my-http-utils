use super::{HttpEnumStructure, HttpObjectStructure, HttpSimpleType};
#[derive(Clone, Debug)]
pub enum ArrayElement {
    SimpleType(HttpSimpleType),
    Object(HttpObjectStructure),
    Enum(HttpEnumStructure),
}
