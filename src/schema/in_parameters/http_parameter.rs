use crate::schema::data_types::{ArrayElement, HttpField, HttpSimpleType};

use super::HttpParameterInputSource;

pub enum NonBodyParameter {
    SimpleType(HttpSimpleType),
    ArrayOf(ArrayElement),
}

#[derive(Debug, Clone)]
pub struct HttpInputParameter {
    pub field: HttpField,
    pub description: String,
    pub source: HttpParameterInputSource,
}

impl HttpInputParameter {
    pub fn is_body_reader(&self) -> bool {
        matches!(self.source, HttpParameterInputSource::BodyModel)
    }

    pub fn is_form_data(&self) -> bool {
        matches!(self.source, HttpParameterInputSource::FormData)
    }

    pub fn is_file_to_upload_from_body(&self) -> bool {
        if self.field.is_file_upload() {
            if let HttpParameterInputSource::BodyRaw = self.source {
                return true;
            }
        }

        false
    }
}
