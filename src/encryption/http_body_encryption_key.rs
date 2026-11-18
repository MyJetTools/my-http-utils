use base64::Engine;
use zeroize::Zeroizing;

use super::aes128gcm::{self, DEFAULT_RECORD_SIZE, SALT_LEN};
use super::{HttpBodyDecryptError, HttpBodyEncryptionKeyError};

const KEY_LEN: usize = 32;

/// A key for `Content-Encoding: aes128gcm` bodies (RFC 8188): 32 random bytes, written as standard
/// base64 (44 characters). Make one with `openssl rand -base64 32`.
///
/// The bytes are wiped when the key drops. Nothing gives them back out: no `Display`, no
/// `Serialize`, no accessor, and `Debug` prints `HttpBodyEncryptionKey(***)`.
///
/// ```
/// use my_http_utils::encryption::{HttpBodyEncryptionKey, AES128GCM_CONTENT_ENCODING};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let (old_text, current_text) = (
/// #     "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=\n",
/// #     "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=\n",
/// # );
/// // The keys come from settings, as `openssl rand -base64 32` printed them.
/// let client_key: HttpBodyEncryptionKey = current_text.parse()?;
///
/// // Client: the request body, sent with `Content-Encoding: aes128gcm`.
/// let request = client_key.encrypt(br#"{"ts":1700000000}"#);
/// assert_eq!(AES128GCM_CONTENT_ENCODING, "aes128gcm");
///
/// // Server: holds an old key and the current one while a key is rotated.
/// let server_keys = vec![
///     HttpBodyEncryptionKey::from_base64(old_text)?,
///     HttpBodyEncryptionKey::from_base64(current_text)?,
/// ];
/// let (key, json) = HttpBodyEncryptionKey::decrypt_with_any(&server_keys, &request)?;
/// assert_eq!(json, br#"{"ts":1700000000}"#);
///
/// // ...and answers with the key that opened the request.
/// let answer = key.encrypt(br#"{"ok":true}"#);
/// assert_eq!(client_key.decrypt(&answer)?, br#"{"ok":true}"#);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct HttpBodyEncryptionKey {
    bytes: Zeroizing<[u8; KEY_LEN]>,
}

impl HttpBodyEncryptionKey {
    /// The length of a key, in bytes.
    pub const LEN: usize = KEY_LEN;

    /// Standard base64 of exactly 32 bytes. Whitespace around it is ignored: a key pasted from a
    /// password manager often ends with a line break.
    pub fn from_base64(text: &str) -> Result<Self, HttpBodyEncryptionKeyError> {
        let text = text.trim();

        // The decoded bytes are the key. They go into a buffer of our own, sized up front so the
        // decoder never moves them, and wiped when it drops: on a decode error too, where
        // base64's own `decode` would drop the bytes it had decoded so far without wiping them.
        let mut bytes = Zeroizing::new(Vec::with_capacity(base64::decoded_len_estimate(
            text.len(),
        )));

        base64::engine::general_purpose::STANDARD
            .decode_vec(text, &mut bytes)
            .map_err(|_| HttpBodyEncryptionKeyError::NotBase64)?;

        Self::from_bytes(&bytes)
    }

    /// Exactly 32 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, HttpBodyEncryptionKeyError> {
        if bytes.len() != KEY_LEN {
            return Err(HttpBodyEncryptionKeyError::WrongLength { len: bytes.len() });
        }

        let mut key = Self {
            bytes: Zeroizing::new([0; KEY_LEN]),
        };
        key.bytes.copy_from_slice(bytes);

        Ok(key)
    }

    /// `plain` encrypted under a fresh random salt, in records of 4096 bytes, with an empty key
    /// id: the body to send with `Content-Encoding:`
    /// [`AES128GCM_CONTENT_ENCODING`](super::AES128GCM_CONTENT_ENCODING). Empty `plain` is still a
    /// valid body: one last record holding only its delimiter.
    ///
    /// # Panics
    ///
    /// If the OS random source gives no salt. A body is never encrypted under a salt that is not
    /// fresh and random.
    pub fn encrypt(&self, plain: &[u8]) -> Vec<u8> {
        self.encrypt_under(&aes128gcm::random_salt(), plain)
    }

    /// `plain` encrypted under `salt`, otherwise as [`encrypt`](Self::encrypt) does it. The caller
    /// answers for the salt being fresh: [`EncryptedBody`](super::EncryptedBody) takes it from its
    /// own caller.
    pub(super) fn encrypt_under(&self, salt: &[u8; SALT_LEN], plain: &[u8]) -> Vec<u8> {
        aes128gcm::encrypt_with(&self.bytes[..], salt, DEFAULT_RECORD_SIZE, &[], plain)
    }

    /// The plain text of `body`. All of the body or nothing: a record that fails its tag, a body
    /// cut short, or anything after the last record refuses the whole body.
    pub fn decrypt(&self, body: &[u8]) -> Result<Vec<u8>, HttpBodyDecryptError> {
        aes128gcm::decrypt(&self.bytes[..], body)
    }

    /// Tries each key in order. Returns the key that opened the body, so the server can answer
    /// with the same key, along with the plain text.
    ///
    /// A wrong key fails on the tag of the first record, so each extra key costs one record, never
    /// the whole body. With no keys, or when no key opens the body, the error is
    /// [`WrongKeyOrDamaged`](HttpBodyDecryptError::WrongKeyOrDamaged). A body that is not
    /// encrypted, or that one key opens but that is cut short or malformed, gives that error
    /// instead: no other key could do better.
    pub fn decrypt_with_any<'k>(
        keys: &'k [HttpBodyEncryptionKey],
        body: &[u8],
    ) -> Result<(&'k HttpBodyEncryptionKey, Vec<u8>), HttpBodyDecryptError> {
        for key in keys {
            match key.decrypt(body) {
                Ok(plain) => return Ok((key, plain)),
                // Another key may open it.
                Err(HttpBodyDecryptError::WrongKeyOrDamaged) => {}
                // Either the same for every key (no aes128gcm header, no records at all), or this
                // key opened a record and the body itself is bad.
                Err(err) => return Err(err),
            }
        }

        Err(HttpBodyDecryptError::WrongKeyOrDamaged)
    }
}

impl std::fmt::Debug for HttpBodyEncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpBodyEncryptionKey(***)")
    }
}

impl std::str::FromStr for HttpBodyEncryptionKey {
    type Err = HttpBodyEncryptionKeyError;

    /// The same as [`HttpBodyEncryptionKey::from_base64`].
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::from_base64(text)
    }
}

impl TryFrom<&str> for HttpBodyEncryptionKey {
    type Error = HttpBodyEncryptionKeyError;

    /// The same as [`HttpBodyEncryptionKey::from_base64`].
    fn try_from(text: &str) -> Result<Self, Self::Error> {
        Self::from_base64(text)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_vectors::*;
    use super::*;

    use HttpBodyDecryptError::*;
    use HttpBodyEncryptionKeyError::*;

    fn our_key() -> HttpBodyEncryptionKey {
        HttpBodyEncryptionKey::from_base64(OUR_KEY).unwrap()
    }

    /// The bytes 0 to 31, the way `Debug` of a byte slice would show them.
    const OUR_KEY_BYTES_AS_TEXT: &str = "0, 1, 2, 3, 4, 5";

    #[test]
    fn the_32_byte_vector_opens_with_the_key() {
        assert_eq!(our_key().decrypt(&base64(OUR_BODY)).unwrap(), OUR_PLAIN);
    }

    #[test]
    fn from_base64_ignores_whitespace_around_the_key() {
        let body = base64(OUR_BODY);

        for text in [
            OUR_KEY.to_string(),
            format!("{OUR_KEY}\n"),
            format!("{OUR_KEY}\r\n"),
            format!("  \t{OUR_KEY} \n"),
        ] {
            let key = HttpBodyEncryptionKey::from_base64(&text).unwrap();
            assert_eq!(key.decrypt(&body).unwrap(), OUR_PLAIN, "{text:?}");
        }
    }

    #[test]
    fn from_str_and_try_from_are_from_base64() {
        let body = base64(OUR_BODY);

        let parsed: HttpBodyEncryptionKey = format!("{OUR_KEY}\n").parse().unwrap();
        assert_eq!(parsed.decrypt(&body).unwrap(), OUR_PLAIN);

        let converted = HttpBodyEncryptionKey::try_from(OUR_KEY).unwrap();
        assert_eq!(converted.decrypt(&body).unwrap(), OUR_PLAIN);

        assert_eq!(
            "not a key".parse::<HttpBodyEncryptionKey>().unwrap_err(),
            NotBase64
        );
        // Base64, of 3 bytes.
        assert_eq!(
            "nope".parse::<HttpBodyEncryptionKey>().unwrap_err(),
            WrongLength { len: 3 }
        );
        assert_eq!(
            HttpBodyEncryptionKey::try_from(RFC_3_1_KEY).unwrap_err(),
            NotBase64
        );
    }

    #[test]
    fn from_base64_refuses_what_is_not_base64() {
        for text in [
            "this is not a key",
            // base64url, not standard base64
            RFC_3_1_KEY,
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwd-h8=",
            // whitespace inside the key is not ignored
            "AAECAwQFBgcICQoLDA0O DxAREhMUFRYXGBkaGxwdHh8=",
        ] {
            assert_eq!(
                HttpBodyEncryptionKey::from_base64(text).unwrap_err(),
                NotBase64,
                "{text:?}"
            );
        }
    }

    #[test]
    fn from_base64_refuses_a_key_of_another_length() {
        for len in [0, 16, 31, 33, 64] {
            let text = to_base64(&data_of_len(len));
            assert_eq!(
                HttpBodyEncryptionKey::from_base64(&text).unwrap_err(),
                WrongLength { len },
                "{len}"
            );
        }
    }

    #[test]
    fn from_bytes_takes_exactly_32_bytes() {
        let key = HttpBodyEncryptionKey::from_bytes(&base64(OUR_KEY)).unwrap();
        assert_eq!(key.decrypt(&base64(OUR_BODY)).unwrap(), OUR_PLAIN);

        for len in [0, 16, 31, 33] {
            assert_eq!(
                HttpBodyEncryptionKey::from_bytes(&data_of_len(len)).unwrap_err(),
                WrongLength { len }
            );
        }
    }

    #[test]
    fn a_key_error_shows_neither_the_input_nor_key_bytes() {
        let short = to_base64(&data_of_len(31));
        let long = to_base64(&data_of_len(33));
        let typo = OUR_KEY.replace('A', "!");

        for text in [short.as_str(), long.as_str(), typo.as_str()] {
            let err = HttpBodyEncryptionKey::from_base64(text).unwrap_err();

            for shown in [format!("{err}"), format!("{err:?}")] {
                assert!(!shown.contains(text), "{shown}");
                assert!(!shown.contains(OUR_KEY_BYTES_AS_TEXT), "{shown}");
            }

            assert!(
                format!("{err}").contains("openssl rand -base64 32"),
                "{err}"
            );
        }

        assert_eq!(
            format!("{}", WrongLength { len: 31 }),
            "the encryption key is 31 bytes and a key is exactly 32: \
             make one with `openssl rand -base64 32`"
        );
    }

    #[test]
    fn debug_of_a_key_does_not_show_it() {
        let shown = format!("{:?}", our_key());

        assert_eq!(shown, "HttpBodyEncryptionKey(***)");
        assert!(!shown.contains(OUR_KEY));
        assert!(!shown.contains(OUR_KEY_BYTES_AS_TEXT));
    }

    #[test]
    fn what_is_encrypted_opens_whatever_its_length() {
        let key = our_key();

        for len in [0, 1, 16, 4079, 4080, 10_000] {
            let plain = data_of_len(len);
            let body = key.encrypt(&plain);

            assert_eq!(key.decrypt(&body).unwrap(), plain, "{len}");
            assert_eq!(key.clone().decrypt(&body).unwrap(), plain, "{len}, cloned");
        }
    }

    /// Records of 4096 bytes and no key id: the header a body from `encrypt` starts with.
    #[test]
    fn encrypt_writes_records_of_4096_and_no_key_id() {
        let key = our_key();

        let body = key.encrypt(b"");
        assert_eq!(&body[SALT_LEN..SALT_LEN + 5], &[0, 0, 0x10, 0, 0]);
        // An empty body is still one last record: the delimiter and the tag.
        assert_eq!(body.len(), SALT_LEN + 5 + 1 + 16);
        assert_eq!(key.decrypt(&body).unwrap(), b"");

        // 4079 bytes fill one record exactly; 4080 need a second one.
        assert_eq!(key.encrypt(&data_of_len(4079)).len(), SALT_LEN + 5 + 4096);
        assert_eq!(
            key.encrypt(&data_of_len(4080)).len(),
            SALT_LEN + 5 + 4096 + 18
        );
    }

    #[test]
    fn a_fresh_salt_goes_into_every_body() {
        let key = our_key();

        let first = key.encrypt(b"same");
        let second = key.encrypt(b"same");

        assert_ne!(first, second);
        assert_ne!(salt_of(&first), salt_of(&second));
    }

    #[test]
    fn plain_data_does_not_decrypt() {
        let key = our_key();

        for plain in [
            &b""[..],
            b"hosts: {}\n",
            OUR_PLAIN,
            b"-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
        ] {
            assert!(key.decrypt(plain).is_err(), "{plain:?}");
        }
    }

    #[test]
    fn another_key_does_not_open_a_body() {
        let body = our_key().encrypt(OUR_PLAIN);
        let other = HttpBodyEncryptionKey::from_bytes(&[0x55; 32]).unwrap();

        assert_eq!(other.decrypt(&body), Err(WrongKeyOrDamaged));
    }

    fn three_keys() -> Vec<HttpBodyEncryptionKey> {
        [1u8, 2, 3]
            .into_iter()
            .map(|byte| HttpBodyEncryptionKey::from_bytes(&[byte; 32]).unwrap())
            .collect()
    }

    #[test]
    fn decrypt_with_any_returns_the_key_that_opened_the_body() {
        let keys = three_keys();
        let body = keys[1].encrypt(OUR_PLAIN);

        let (opened_by, plain) = HttpBodyEncryptionKey::decrypt_with_any(&keys, &body).unwrap();

        assert!(std::ptr::eq(opened_by, &keys[1]));
        assert_eq!(plain, OUR_PLAIN);

        // The answer goes back under the same key, and the client's key opens it.
        let answer = opened_by.encrypt(b"ok");
        assert_eq!(keys[1].decrypt(&answer).unwrap(), b"ok");
    }

    #[test]
    fn decrypt_with_any_fails_when_no_key_opens_the_body() {
        let keys = three_keys();
        let body = our_key().encrypt(OUR_PLAIN);

        assert_eq!(
            HttpBodyEncryptionKey::decrypt_with_any(&keys, &body).unwrap_err(),
            WrongKeyOrDamaged
        );
        assert_eq!(
            HttpBodyEncryptionKey::decrypt_with_any(&[], &body).unwrap_err(),
            WrongKeyOrDamaged
        );
    }

    #[test]
    fn decrypt_with_any_tells_why_the_right_key_did_not_open_it() {
        let keys = three_keys();

        assert_eq!(
            HttpBodyEncryptionKey::decrypt_with_any(&keys, b"hosts: {}\n").unwrap_err(),
            NotEncrypted
        );

        // A body of the third key that lost its last record.
        let body = keys[2].encrypt(&data_of_len(5000));
        let cut = &body[..SALT_LEN + 5 + 4096];
        assert_eq!(
            HttpBodyEncryptionKey::decrypt_with_any(&keys, cut).unwrap_err(),
            CutShort
        );
    }
}
