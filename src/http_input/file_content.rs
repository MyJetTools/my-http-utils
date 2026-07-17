/// An uploaded file read from a `multipart/form-data` part: its content plus metadata. Ported
/// from `my-http-server-core::types::FileContent`. The `TryInto<FileContent>` conversion lives
/// with the `server`-gated `HttpInputValue` (only the `FormData` variant can yield a file).
pub struct FileContent {
    pub content_type: String,
    pub file_name: String,
    pub content: Vec<u8>,
}
