use super::{ArrayElement, HttpDataType, HttpField};
#[derive(Clone, Debug)]
pub struct HttpObjectFields {
    pub struct_id: String,
    pub fields: Vec<HttpField>,
}

#[derive(Clone, Debug)]
pub struct HttpObjectStructure {
    pub main: HttpObjectFields,
    pub generic: Option<HttpObjectFields>,
}

impl super::InputStructure for HttpObjectStructure {
    fn get_struct_id(&self) -> String {
        match &self.generic {
            Some(generic_data) => format!(
                "{}_{}",
                self.main.struct_id.as_str(),
                generic_data.struct_id.as_str()
            ),
            None => self.main.struct_id.clone(),
        }
    }
}

impl HttpObjectStructure {
    pub fn into_http_data_type_object(self) -> HttpDataType {
        HttpDataType::Object(self)
    }

    pub fn into_http_data_type_array(self) -> HttpDataType {
        HttpDataType::ArrayOf(ArrayElement::Object(self))
    }

    pub fn new(struct_id: &'static str, generic_struct_id: Option<String>) -> Self {
        let generic = generic_struct_id.map(|generic_struct_id| HttpObjectFields {
                struct_id: generic_struct_id,
                fields: vec![],
            });

        Self {
            main: HttpObjectFields {
                struct_id: struct_id.into(),
                fields: vec![],
            },
            generic,
        }
    }
}
