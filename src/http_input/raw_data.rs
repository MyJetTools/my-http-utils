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

/// The whole `#[http_body_raw]` body, verbatim — **infallible**. It's a `From`, so std's blanket
/// gives `TryFrom<Vec<u8>>` with `Error = Infallible`, which the derive's uniform `.try_into()?`
/// picks up (unified into `HttpParseError` via `From<Infallible>`). RawData never fails to build.
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
