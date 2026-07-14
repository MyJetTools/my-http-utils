use rust_extensions::StrOrString;

use super::HttpDataType;

#[derive(Clone, Debug)]
pub struct HttpField {
    pub name: String,
    pub data_type: HttpDataType,
    pub required: bool,
}

impl HttpField {
    pub fn new(name: &str, data_type: HttpDataType, required: bool) -> Self {
        Self {
            name: name.to_string(),
            data_type,
            required,
        }
    }

    pub fn is_file_upload(&self) -> bool {
        self.data_type.is_binary()
    }

    pub fn get_query_field_name<'s>(&'s self) -> StrOrString<'s> {
        match &self.data_type {
            HttpDataType::SimpleType(_) => StrOrString::create_as_str(self.name.as_str()),
            HttpDataType::Object(_) => StrOrString::create_as_str(self.name.as_str()),
            HttpDataType::ArrayOf(_) => {
                StrOrString::create_as_string(format!("{}[]", self.name.as_str()))
            }
            HttpDataType::DictionaryOf(_) => StrOrString::create_as_str(self.name.as_str()),
            HttpDataType::DictionaryOfArray(_) => StrOrString::create_as_str(self.name.as_str()),
            HttpDataType::Enum(_) => StrOrString::create_as_str(self.name.as_str()),
            HttpDataType::None => StrOrString::create_as_str(self.name.as_str()),
        }
    }
}
