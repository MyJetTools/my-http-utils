use super::HttpDataType;
#[derive(Clone, Debug)]
pub struct HttpEnumCase {
    pub id: i16,
    pub value: &'static str,
    pub description: &'static str,
}
#[derive(Clone, Debug)]
pub enum EnumType {
    Integer,
    String,
}
#[derive(Clone, Debug)]
pub struct HttpEnumStructure {
    pub struct_id: &'static str,
    pub enum_type: EnumType,
    pub cases: Vec<HttpEnumCase>,
}

impl super::InputStructure for HttpEnumStructure {
    fn get_struct_id(&self) -> String {
        self.struct_id.to_string()
    }
}

impl HttpEnumStructure {
    pub fn into_http_data_type_object(self) -> HttpDataType {
        HttpDataType::Enum(self)
    }
}

impl From<HttpEnumStructure> for HttpDataType {
    fn from(val: HttpEnumStructure) -> Self {
        val.into_http_data_type_object()
    }
}
