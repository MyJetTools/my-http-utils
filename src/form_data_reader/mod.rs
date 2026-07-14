mod content_disposition_parser;
mod content_iterator;
mod form_data_item;
mod form_data_reader;
pub use content_disposition_parser::*;
pub use form_data_item::*;
pub use form_data_reader::*;
pub mod mappers;
#[derive(Debug)]
pub enum ReadingFromDataError {
    ParameterMissing(String),
    ValidationError { field: String, error: String },
}
