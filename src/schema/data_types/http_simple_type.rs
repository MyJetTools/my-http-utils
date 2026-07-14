#[derive(Clone, Debug)]
pub enum HttpSimpleType {
    Integer,
    Long,
    Float,
    Double,
    String,
    Byte,
    Binary,
    Boolean,
    Date,
    DateTime,
    Password,
}

impl HttpSimpleType {
    pub fn as_format(&self) -> &str {
        match self {
            HttpSimpleType::Integer => "int32",
            HttpSimpleType::Long => "int64",
            HttpSimpleType::Float => "float",
            HttpSimpleType::Double => "double",
            HttpSimpleType::String => "string",
            HttpSimpleType::Byte => "byte",
            HttpSimpleType::Binary => "binary",
            HttpSimpleType::Boolean => "boolean",
            HttpSimpleType::Date => "date",
            HttpSimpleType::DateTime => "date-time",
            HttpSimpleType::Password => "password",
        }
    }

    pub fn as_swagger_type(&self) -> &str {
        match self {
            HttpSimpleType::Integer => "integer",
            HttpSimpleType::Long => "integer",
            HttpSimpleType::Float => "number",
            HttpSimpleType::Double => "number",
            HttpSimpleType::String => "string",
            HttpSimpleType::Byte => "integer",
            HttpSimpleType::Binary => "string",
            HttpSimpleType::Boolean => "boolean",
            HttpSimpleType::Date => "string",
            HttpSimpleType::DateTime => "string",
            HttpSimpleType::Password => "string",
        }
    }

    pub fn is_binary(&self) -> bool {
        matches!(self, HttpSimpleType::Binary)
    }
}
