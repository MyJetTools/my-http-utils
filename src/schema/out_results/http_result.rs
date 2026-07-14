use crate::schema::data_types::{HttpDataType, HttpObjectStructure};

use super::IntoHttpResult;

#[derive(Clone)]
pub struct HttpResult {
    pub http_code: u16,
    pub nullable: bool,
    pub description: String,
    pub data_type: HttpDataType,
}

impl IntoHttpResult for HttpObjectStructure {
    fn into_http_result_object(
        self,
        http_code: u16,
        nullable: bool,
        description: &str,
    ) -> HttpResult {
        HttpResult {
            http_code,
            nullable,
            description: description.to_string(),
            data_type: self.into_http_data_type_object(),
        }
    }

    fn into_http_result_array(
        self,
        http_code: u16,
        nullable: bool,
        description: &str,
    ) -> HttpResult {
        HttpResult {
            http_code,
            nullable,
            description: description.to_string(),
            data_type: self.into_http_data_type_array(),
        }
    }
}
