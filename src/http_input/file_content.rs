use crate::body::FormDataBody;
use crate::form_data_reader::FormDataHeader;

use super::{FormDataProperty, HttpParseError};

/// What RFC 7578 says a part is when it comes without a `Content-Type`.
const DEFAULT_PART_CONTENT_TYPE: &str = "text/plain";

/// An uploaded file read from a `multipart/form-data` part: its content plus metadata. Ported
/// from `my-http-server-core::types::FileContent`.
///
/// A `#[http_form_data]` field type (`FileContent`, `Option<FileContent>`, `Vec<FileContent>`):
/// read out of the part's header (`filename`, `Content-Type`) and content, and sent as a file part
/// — see [`FormDataProperty`]. A part with no `filename` is not a file, and reading one into a
/// `FileContent` is an error. A file part with no `Content-Type` reads as `text/plain`, and an
/// empty `content_type` is sent as `application/octet-stream`.
///
/// The `TryInto<FileContent>` conversion of the `server`-gated `HttpInputValue` stays for code that
/// reads a value by hand.
#[derive(Clone, PartialEq, Eq)]
pub struct FileContent {
    pub content_type: String,
    pub file_name: String,
    pub content: Vec<u8>,
}

/// The content is shown by its length only: a `{:?}` of a model holding an upload would otherwise
/// put the whole file into a log line.
impl std::fmt::Debug for FileContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileContent")
            .field("content_type", &self.content_type)
            .field("file_name", &self.file_name)
            .field("content", &format_args!("<{} bytes>", self.content.len()))
            .finish()
    }
}

impl FormDataProperty for FileContent {
    fn from_form_data_content(
        header: FormDataHeader<'_>,
        content: &[u8],
    ) -> Result<Self, HttpParseError> {
        let Some(file_name) = header.file_name else {
            return Err(HttpParseError::NotSupportedContentType(format!(
                "Field '{}' is a form-data value, not a file",
                header.name
            )));
        };

        Ok(Self {
            content_type: header
                .content_type
                .unwrap_or(DEFAULT_PART_CONTENT_TYPE)
                .to_string(),
            file_name: file_name.to_string(),
            content: content.to_vec(),
        })
    }

    fn write_form_data(&self, name: &'static str, form_data: FormDataBody) -> FormDataBody {
        form_data.append_form_data_file(
            name,
            self.file_name.clone(),
            self.content_type.clone(),
            &self.content,
        )
    }
}

/// Schema (server-only). A file is described as OpenAPI `binary` (`type: string, format: binary`),
/// which is what makes Swagger UI offer a file picker for the field — the same description
/// `RawData` gets. Gated on `server` like the rest of the schema layer.
#[cfg(feature = "server")]
impl crate::schema::data_types::DataTypeProvider for FileContent {
    fn get_data_type() -> crate::schema::data_types::HttpDataType {
        crate::schema::data_types::HttpDataType::SimpleType(
            crate::schema::data_types::HttpSimpleType::Binary,
        )
    }
}
