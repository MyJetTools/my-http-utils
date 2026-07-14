use super::{ArrayElement, HttpEnumStructure, HttpObjectStructure, HttpSimpleType};

#[derive(Debug, Clone)]
pub enum HttpDataType {
    SimpleType(HttpSimpleType),
    Object(HttpObjectStructure),
    ArrayOf(ArrayElement),
    DictionaryOf(ArrayElement),
    DictionaryOfArray(ArrayElement),
    Enum(HttpEnumStructure),
    None,
}

impl HttpDataType {
    pub fn is_array(&self) -> bool {
        matches!(self, HttpDataType::ArrayOf(_))
    }

    pub fn is_none(&self) -> bool {
        matches!(self, HttpDataType::None)
    }

    pub fn is_simple_type(&self) -> bool {
        matches!(self, HttpDataType::SimpleType(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, HttpDataType::Object(_))
    }

    pub fn as_string() -> Self {
        Self::SimpleType(HttpSimpleType::String)
    }

    pub fn as_long() -> Self {
        Self::SimpleType(HttpSimpleType::Long)
    }

    pub fn as_float() -> Self {
        Self::SimpleType(HttpSimpleType::Float)
    }

    pub fn as_double() -> Self {
        Self::SimpleType(HttpSimpleType::Double)
    }

    pub fn as_binary() -> Self {
        Self::SimpleType(HttpSimpleType::Binary)
    }

    pub fn as_date() -> Self {
        Self::SimpleType(HttpSimpleType::Date)
    }

    pub fn as_date_time() -> Self {
        Self::SimpleType(HttpSimpleType::DateTime)
    }

    pub fn as_bool() -> Self {
        Self::SimpleType(HttpSimpleType::Boolean)
    }

    pub fn as_boolean() -> Self {
        Self::SimpleType(HttpSimpleType::Boolean)
    }

    pub fn as_password() -> Self {
        Self::SimpleType(HttpSimpleType::Password)
    }

    pub fn as_array_simple_type(simple_type: HttpSimpleType) -> Self {
        Self::ArrayOf(ArrayElement::SimpleType(simple_type))
    }

    pub fn as_array_of_object(object: HttpObjectStructure) -> Self {
        Self::ArrayOf(ArrayElement::Object(object))
    }

    pub fn is_binary(&self) -> bool {
        matches!(self, HttpDataType::SimpleType(HttpSimpleType::Binary))
    }
}
