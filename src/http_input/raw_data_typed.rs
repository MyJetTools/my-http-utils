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

/// The whole `#[http_body_raw]` body, verbatim — **infallible to build**: it only stores the raw
/// bytes. Unlike [`RawData`], this type *can* surface a JSON error, but only later and only from
/// [`Self::deserialize_json`] — that is the single place `T` is actually parsed. (`From`, so the
/// derive's uniform `.try_into()?` uses std's `TryFrom<Vec<u8>>` with `Error = Infallible`.)
impl<T: DeserializeOwned> From<Vec<u8>> for RawDataTyped<T> {
    fn from(data: Vec<u8>) -> Self {
        Self::new(data, SRC_BODY)
    }
}

impl<T: DeserializeOwned> AsRef<[u8]> for RawDataTyped<T> {
    fn as_ref(&self) -> &[u8] {
        self.data.as_ref()
    }
}
