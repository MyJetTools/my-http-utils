use rust_extensions::date_time::DateTimeAsMicroseconds;
use zeroize::Zeroizing;

use super::aes128gcm::{self, SALT_LEN};
use super::{HttpBodyDecryptError, HttpBodyEncryptionKey};

/// The timestamp the plain text starts with: an `i64` of unix microseconds, little-endian.
const TIMESTAMP_LEN: usize = 8;

/// A body with the time it was sent and the salt it is encrypted under.
///
/// Encrypted, it is an ordinary `Content-Encoding: aes128gcm` body (records of 4096 bytes, no key
/// id) whose plain text starts with the timestamp:
///
/// ```text
/// plain text = timestamp (i64, unix microseconds, little-endian: 8 bytes) || body
/// encrypted  = salt || rs || idlen || records      (the aes128gcm header starts with `salt`)
/// ```
///
/// An empty body is not encrypted at all: it is empty bytes, both ways.
///
/// The salt and the timestamp are what a server needs to refuse a replayed body: it refuses a
/// timestamp too far from its own clock, and a salt it has already seen within that time. That
/// checking is the server's job; this type only carries them.
///
/// `Debug` prints the length of the body, never the body: it is the plain text of something worth
/// encrypting.
///
/// ```
/// use my_http_utils::encryption::{EncryptedBody, HttpBodyEncryptionKey};
/// use my_http_utils::rust_extensions::date_time::DateTimeAsMicroseconds;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let key_text = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=\n";
/// let key: HttpBodyEncryptionKey = key_text.parse()?;
///
/// // The sender: a fresh salt, and its own clock.
/// let bytes = EncryptedBody {
///     salt: EncryptedBody::random_salt(),
///     timestamp: DateTimeAsMicroseconds::now(),
///     body: br#"{"ok":true}"#.to_vec(),
/// }
/// .encrypt(&key);
///
/// // The receiver: the body, and the salt and the time to refuse a replay by.
/// let received = EncryptedBody::decrypt(&bytes, &key)?;
/// assert_eq!(received.body, br#"{"ok":true}"#);
/// assert_eq!(received.salt, bytes[..16]);
///
/// // An empty body is not encrypted: it is empty bytes.
/// let empty = EncryptedBody { body: Vec::new(), ..received };
/// assert!(empty.encrypt(&key).is_empty());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct EncryptedBody {
    /// The salt of the encrypted body: its first 16 bytes. **Never encrypt twice under one salt
    /// with one key**: two bodies under the same salt and key share their AES-GCM nonces, which
    /// gives both plain texts away and lets anyone forge a body. Take a fresh one from
    /// [`random_salt`](Self::random_salt) for every body.
    pub salt: [u8; 16],
    /// When the body was sent, by the sender's clock.
    pub timestamp: DateTimeAsMicroseconds,
    /// The body itself, in plain text.
    pub body: Vec<u8>,
}

impl EncryptedBody {
    /// A fresh salt from the OS random source, for a body about to be encrypted.
    ///
    /// # Panics
    ///
    /// If the OS random source gives nothing.
    pub fn random_salt() -> [u8; 16] {
        aes128gcm::random_salt()
    }

    /// The timestamp and the body, encrypted with `key` under [`salt`](Self::salt): the body to
    /// send with `Content-Encoding:` [`AES128GCM_CONTENT_ENCODING`](super::AES128GCM_CONTENT_ENCODING).
    ///
    /// An empty body gives empty bytes, with no timestamp in them: there is nothing to encrypt.
    pub fn encrypt(&self, key: &HttpBodyEncryptionKey) -> Vec<u8> {
        if self.body.is_empty() {
            return Vec::new();
        }

        // The plain text is secret: this copy of it is wiped when it drops.
        let mut plain = Zeroizing::new(Vec::with_capacity(TIMESTAMP_LEN + self.body.len()));
        plain.extend_from_slice(&self.timestamp.unix_microseconds.to_le_bytes());
        plain.extend_from_slice(&self.body);

        key.encrypt_under(&self.salt, &plain)
    }

    /// What [`encrypt`](Self::encrypt) made of it: the salt, the timestamp and the body, decrypted
    /// with `key`.
    ///
    /// Empty bytes are an empty body. Nothing was encrypted, so there is no salt and no time to
    /// give back: the salt is all zeros and the timestamp is 0, the start of 1970, which a server
    /// that wants a recent timestamp refuses. They open with no key at all, so an empty body
    /// proves nothing about who sent it.
    ///
    /// A body that opens but holds less than its 8-byte timestamp is
    /// [`NoTimestamp`](HttpBodyDecryptError::NoTimestamp). Anything else is as
    /// [`HttpBodyEncryptionKey::decrypt`] has it: all of the body or nothing.
    pub fn decrypt(
        bytes: &[u8],
        key: &HttpBodyEncryptionKey,
    ) -> Result<Self, HttpBodyDecryptError> {
        if bytes.is_empty() {
            return Ok(Self {
                salt: [0; SALT_LEN],
                timestamp: DateTimeAsMicroseconds::new(0),
                body: Vec::new(),
            });
        }

        // An aes128gcm body starts with its salt.
        let salt = *bytes
            .first_chunk::<SALT_LEN>()
            .ok_or(HttpBodyDecryptError::NotEncrypted)?;

        // Wiped when it drops for as long as it is ours: a body refused for its length leaves
        // none of its plain text behind.
        let mut plain = Zeroizing::new(key.decrypt(bytes)?);

        let timestamp = plain
            .first_chunk::<TIMESTAMP_LEN>()
            .ok_or(HttpBodyDecryptError::NoTimestamp)?;
        let timestamp = DateTimeAsMicroseconds::new(i64::from_le_bytes(*timestamp));

        plain.drain(..TIMESTAMP_LEN);

        Ok(Self {
            salt,
            timestamp,
            // Moves the buffer out, it does not copy it: the empty `Vec` left behind is what drops.
            body: std::mem::take(&mut *plain),
        })
    }
}

impl std::fmt::Debug for EncryptedBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedBody")
            .field("salt", &self.salt)
            .field("timestamp", &self.timestamp)
            .field("body", &format_args!("<{} bytes>", self.body.len()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_vectors::*;
    use super::*;

    use HttpBodyDecryptError::*;

    fn our_key() -> HttpBodyEncryptionKey {
        HttpBodyEncryptionKey::from_base64(OUR_KEY).unwrap()
    }

    /// Any 16 bytes will do here; a real body takes `EncryptedBody::random_salt()`.
    const SALT: [u8; 16] = [
        0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae,
        0xaf,
    ];

    /// Every byte different, so the order of the bytes shows.
    const TIMESTAMP: i64 = 0x0102_0304_0506_0708;
    const TIMESTAMP_LITTLE_ENDIAN: [u8; 8] = [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01];

    fn sample(body: &[u8]) -> EncryptedBody {
        EncryptedBody {
            salt: SALT,
            timestamp: DateTimeAsMicroseconds::new(TIMESTAMP),
            body: body.to_vec(),
        }
    }

    #[test]
    fn the_plain_text_is_the_timestamp_little_endian_then_the_body() {
        let bytes = sample(OUR_PLAIN).encrypt(&our_key());

        assert_eq!(
            bytes[..16],
            SALT,
            "the aes128gcm header starts with the salt"
        );

        let plain = our_key().decrypt(&bytes).unwrap();
        assert_eq!(plain[..8], TIMESTAMP_LITTLE_ENDIAN);
        assert_eq!(plain[8..], *OUR_PLAIN);
    }

    #[test]
    fn decrypt_gives_back_the_salt_the_timestamp_and_the_body() {
        // A record of 4096 bytes carries 4079 bytes of plain text, and the 8 of the timestamp
        // move where the body is cut: one record, exactly full, two records.
        for len in [1, 100, 4071, 4072, 4079, 4080, 10_000] {
            let sent = sample(&data_of_len(len));
            let received = EncryptedBody::decrypt(&sent.encrypt(&our_key()), &our_key()).unwrap();
            assert_eq!(received, sent, "{len} bytes");
        }
    }

    #[test]
    fn any_timestamp_comes_back() {
        let now = DateTimeAsMicroseconds::now().unix_microseconds;

        for unix_microseconds in [0, 1, -1, i64::MIN, i64::MAX, now] {
            let sent = EncryptedBody {
                timestamp: DateTimeAsMicroseconds::new(unix_microseconds),
                ..sample(OUR_PLAIN)
            };
            let received = EncryptedBody::decrypt(&sent.encrypt(&our_key()), &our_key()).unwrap();
            assert_eq!(
                received.timestamp.unix_microseconds, unix_microseconds,
                "{unix_microseconds}"
            );
        }
    }

    #[test]
    fn an_empty_body_is_empty_bytes() {
        assert_eq!(sample(b"").encrypt(&our_key()), Vec::<u8>::new());
    }

    #[test]
    fn empty_bytes_are_an_empty_body_with_no_salt_and_no_time() {
        assert_eq!(
            EncryptedBody::decrypt(&[], &our_key()),
            Ok(EncryptedBody {
                salt: [0; 16],
                timestamp: DateTimeAsMicroseconds::new(0),
                body: Vec::new(),
            })
        );
    }

    #[test]
    fn a_body_shorter_than_its_timestamp_is_refused() {
        for plain in [&b""[..], b"1234567"] {
            let bytes = our_key().encrypt(plain);
            assert_eq!(
                EncryptedBody::decrypt(&bytes, &our_key()),
                Err(NoTimestamp),
                "{plain:?}"
            );
        }
    }

    #[test]
    fn a_timestamp_alone_is_an_empty_body() {
        let bytes = our_key().encrypt(&TIMESTAMP_LITTLE_ENDIAN);

        assert_eq!(
            EncryptedBody::decrypt(&bytes, &our_key()),
            Ok(EncryptedBody {
                salt: salt_of(&bytes),
                timestamp: DateTimeAsMicroseconds::new(TIMESTAMP),
                body: Vec::new(),
            })
        );
    }

    #[test]
    fn another_key_or_a_damaged_body_is_refused() {
        let bytes = sample(OUR_PLAIN).encrypt(&our_key());

        let other_key = HttpBodyEncryptionKey::from_bytes(&[7; 32]).unwrap();
        assert_eq!(
            EncryptedBody::decrypt(&bytes, &other_key),
            Err(WrongKeyOrDamaged)
        );

        // The salt, the first record and the last byte. Not the record size: aes128gcm leaves it
        // unauthenticated.
        for at in [0, 15, 21, bytes.len() - 1] {
            let mut damaged = bytes.clone();
            damaged[at] ^= 0x01;
            assert_eq!(
                EncryptedBody::decrypt(&damaged, &our_key()),
                Err(WrongKeyOrDamaged),
                "byte {at}"
            );
        }
    }

    #[test]
    fn bytes_that_are_not_encrypted_are_refused() {
        for bytes in [&b"{}"[..], OUR_PLAIN] {
            assert_eq!(
                EncryptedBody::decrypt(bytes, &our_key()),
                Err(NotEncrypted),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn the_salt_alone_decides_the_bytes() {
        // Why a salt is never used twice with one key: the same salt gives the same bytes.
        let bytes = sample(OUR_PLAIN).encrypt(&our_key());
        assert_eq!(sample(OUR_PLAIN).encrypt(&our_key()), bytes);

        let other_salt = EncryptedBody {
            salt: EncryptedBody::random_salt(),
            ..sample(OUR_PLAIN)
        };
        assert_ne!(other_salt.encrypt(&our_key()), bytes);
    }

    #[test]
    fn random_salts_are_fresh() {
        assert_ne!(EncryptedBody::random_salt(), EncryptedBody::random_salt());
    }

    #[test]
    fn debug_shows_how_long_the_body_is_never_the_body() {
        let body = EncryptedBody {
            timestamp: DateTimeAsMicroseconds::now(),
            ..sample(b"password=hunter2")
        };
        let text = format!("{body:?}");

        assert!(text.starts_with("EncryptedBody {"), "{text}");
        assert!(text.contains("<16 bytes>"), "{text}");
        // `Debug` of the bytes themselves would show them as numbers: "h", "u", "n".
        assert!(!text.contains("104, 117, 110"), "{text}");
        assert!(!text.contains("hunter2"), "{text}");
    }
}
