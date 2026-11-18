//! Encrypted HTTP bodies: `Content-Encoding: aes128gcm`, RFC 8188 ("Encrypted Content-Encoding
//! for HTTP"). Behind the `encryption` feature.
//!
//! One implementation for the whole stack, so every crate encrypts and decrypts the same way:
//!
//! - a client (fl-url) encrypts a request body with [`HttpBodyEncryptionKey::encrypt`] and sends it
//!   with `Content-Encoding:` [`AES128GCM_CONTENT_ENCODING`];
//! - a server (my-http-server) opens it with [`HttpBodyEncryptionKey::decrypt_with_any`], trying
//!   every key it holds, and encrypts its answer with the key that call hands back;
//! - a service that serves data to my-reverse-proxy encrypts its answers with its current key.
//!
//! A key is 32 random bytes written as standard base64, 44 characters: `openssl rand -base64 32`.
//! A body is `salt(16) || rs(u32 BE) || idlen(1) || keyid || records`: the content key and the
//! nonces come from the key and the salt through HKDF-SHA-256, and every record is AES-128-GCM.
//!
//! The standard, byte by byte, with the test vectors every implementation passes (the tests here
//! are the same ones): `docs/encrypted-service-data.md` in
//! [my-reverse-proxy](https://github.com/my-jet-tools/my-reverse-proxy/blob/main/docs/encrypted-service-data.md).
//!
//! Not here, on purpose: streaming a body that comes in chunks, and replay protection (a `ts` in
//! the plain text, remembered salts). Replay protection is the server's job.

mod aes128gcm;
mod errors;
mod http_body_encryption_key;
#[cfg(test)]
mod test_vectors;

pub use errors::*;
pub use http_body_encryption_key::*;

/// The value of `Content-Encoding` (and `Accept-Encoding`) for this encryption.
pub const AES128GCM_CONTENT_ENCODING: &str = "aes128gcm";
