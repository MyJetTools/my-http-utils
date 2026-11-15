/// Raw bytes of a value/body, handed to the model untouched. Ported from
/// `my-http-server-core::types::RawData`.
pub struct RawData(Vec<u8>);

impl RawData {
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    pub fn from_slice(data: &[u8]) -> Self {
        Self(data.to_vec())
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl From<RawData> for Vec<u8> {
    fn from(value: RawData) -> Self {
        value.0
    }
}

/// Build `RawData` from the whole body bytes — **infallible**. (The derive's server parse builds a
/// `#[http_body_raw] RawData` field through the `server`-gated `core::FromRawBody`, which calls
/// `RawData::new`; this `From` stays for direct `Vec<u8>` → `RawData` conversions.)
impl From<Vec<u8>> for RawData {
    fn from(data: Vec<u8>) -> Self {
        Self::new(data)
    }
}

impl AsRef<[u8]> for RawData {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// A `#[http_form_data]` field that keeps a part's content and nothing else — any part, a file or a
/// value, whatever its bytes. Sent as a file part, which is what its `binary` schema promises a
/// receiver: named `blob` (what a browser names a nameless `Blob`), typed
/// `application/octet-stream`. Use [`FileContent`](super::FileContent) to keep the file name and
/// the content type.
impl super::FormDataProperty for RawData {
    fn from_form_data_content(
        _header: crate::form_data_reader::FormDataHeader<'_>,
        content: &[u8],
    ) -> Result<Self, super::HttpParseError> {
        Ok(Self::from_slice(content))
    }

    fn write_form_data(
        &self,
        name: &'static str,
        form_data: crate::body::FormDataBody,
    ) -> crate::body::FormDataBody {
        form_data.append_form_data_file(
            name,
            "blob",
            crate::body::DEFAULT_FILE_CONTENT_TYPE,
            self.as_slice(),
        )
    }
}

/// Schema (server-only). A raw byte body carries no inner model, so it is described as OpenAPI
/// `binary` (`type: string, format: binary`) — the standard shape for an arbitrary-bytes body, and
/// what makes `#[http_body_raw] body: RawData` produce a schema at all (the derive calls
/// `RawData::get_data_type()`). Gated on `server` like the rest of the schema layer.
#[cfg(feature = "server")]
impl crate::schema::data_types::DataTypeProvider for RawData {
    fn get_data_type() -> crate::schema::data_types::HttpDataType {
        crate::schema::data_types::HttpDataType::SimpleType(
            crate::schema::data_types::HttpSimpleType::Binary,
        )
    }
}
