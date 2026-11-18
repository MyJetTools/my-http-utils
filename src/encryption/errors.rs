/// Why a text or a byte string is not an
/// [`HttpBodyEncryptionKey`](super::HttpBodyEncryptionKey).
///
/// It never carries, nor prints, what it was given: that may well be a real key with a typo in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpBodyEncryptionKeyError {
    /// Not standard base64 (the `+` `/` alphabet, with its `=` padding).
    NotBase64,
    /// Not 32 bytes: `len` is how many there are.
    WrongLength { len: usize },
}

impl std::fmt::Display for HttpBodyEncryptionKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBase64 => f.write_str(
                "the encryption key is not standard base64: a key is 32 random bytes in base64, \
                 44 characters; make one with `openssl rand -base64 32`",
            ),
            Self::WrongLength { len } => write!(
                f,
                "the encryption key is {len} bytes and a key is exactly 32: \
                 make one with `openssl rand -base64 32`"
            ),
        }
    }
}

impl std::error::Error for HttpBodyEncryptionKeyError {}

/// Why a body does not decrypt. The meanings are those of `OpenError` in my-reverse-proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpBodyDecryptError {
    /// Too short, or a header no aes128gcm body has: the body is not encrypted this way.
    NotEncrypted,
    /// A record does not open: the key is another one, or the body is damaged. Also what
    /// [`decrypt_with_any`](super::HttpBodyEncryptionKey::decrypt_with_any) gives with no keys.
    WrongKeyOrDamaged,
    /// The body ends without its last record (the one whose delimiter is `0x02`): it is cut short.
    CutShort,
    /// Something follows the last record, or a record has no delimiter.
    Malformed,
}

impl std::fmt::Display for HttpBodyDecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::NotEncrypted => "not an aes128gcm body",
            Self::WrongKeyOrDamaged => {
                "a record does not open: the key is wrong or the body is damaged"
            }
            Self::CutShort => "the body ends without its last record: it is cut short",
            Self::Malformed => "the records are not laid out as aes128gcm lays them out",
        };

        f.write_str(text)
    }
}

impl std::error::Error for HttpBodyDecryptError {}
