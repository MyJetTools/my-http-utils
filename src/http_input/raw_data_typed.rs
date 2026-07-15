use std::marker::PhantomData;

use serde::de::DeserializeOwned;

use crate::http_input::core::convert_from_str;
use crate::http_input::core::data_src::SRC_BODY;
use crate::http_input::HttpParseError;

/// Raw bytes that additionally know they can be deserialized into `T` on demand. Ported from
/// `my-http-server-core::types::RawDataTyped`.
pub struct RawDataTyped<T: DeserializeOwned> {
    data: Vec<u8>,
    ty: PhantomData<T>,
    src: &'static str,
}

impl<T: DeserializeOwned> RawDataTyped<T> {
    pub fn new(data: Vec<u8>, src: &'static str) -> Self {
        Self {
            data,
            ty: PhantomData,
            src,
        }
    }

    pub fn from_slice(data: &[u8], src: &'static str) -> Self {
        Self {
            data: data.to_vec(),
            ty: PhantomData,
            src,
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub fn deserialize_json(&self) -> Result<T, HttpParseError> {
        convert_from_str::to_json_from_slice("RawDataTyped", &self.data, self.src)
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.data
    }
}

impl<T: DeserializeOwned> From<RawDataTyped<T>> for Vec<u8> {
    fn from(value: RawDataTyped<T>) -> Self {
        value.data
    }
}

/// Client direction: build the typed raw body straight from a `T` (`payload.into()`), serialising
/// it into the JSON bytes the request will carry. Serialisation of a well-formed model never fails,
/// so a failure is a programmer error (a broken `Serialize`) — we panic rather than thread an error
/// no caller can sensibly handle. The *server* direction (whole body `Vec<u8>` → this type) is the
/// crate-local [`crate::http_input::core::FromRawBody`], so this `From<T>` does not clash with it.
impl<T: serde::Serialize + DeserializeOwned> From<T> for RawDataTyped<T> {
    fn from(value: T) -> Self {
        let data = serde_json::to_vec(&value)
            .expect("RawDataTyped<T>: T failed to serialize into the request body");
        Self::new(data, SRC_BODY)
    }
}

impl<T: DeserializeOwned> AsRef<[u8]> for RawDataTyped<T> {
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}

/// Schema (server-only). Lets `#[http_body_raw] body: RawDataTyped<Model>` drive OpenAPI: the
/// derive's schema codegen calls `<FieldType>::get_data_type()`, and for a typed raw body the body
/// param's data type is simply the inner model's — so Swagger renders `Model`'s structure. Gated on
/// `server` because `DataTypeProvider` / `HttpDataType` only exist there.
#[cfg(feature = "server")]
impl<T: DeserializeOwned + crate::schema::data_types::DataTypeProvider>
    crate::schema::data_types::DataTypeProvider for RawDataTyped<T>
{
    fn get_data_type() -> crate::schema::data_types::HttpDataType {
        T::get_data_type()
    }
}
