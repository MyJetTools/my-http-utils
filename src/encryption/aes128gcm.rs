//! `Content-Encoding: aes128gcm`, RFC 8188: the low level, crate-private.
//!
//! These functions take the input keying material (IKM) as bytes of any length, because the
//! RFC 8188 vectors use 16-byte keys; the public [`HttpBodyEncryptionKey`](super::HttpBodyEncryptionKey)
//! is always 32 bytes and calls into here.
//!
//! ```text
//! body   = header || record_0 || ... || record_last
//! header = salt (16) || rs (u32, big-endian) || idlen (1) || keyid (idlen)
//! ```
//!
//! `CEK` and `NONCE_BASE` come from HKDF-SHA-256 (the salt, the IKM). A record is
//! `AES-128-GCM(CEK, NONCE_BASE xor i, data || delimiter || zero padding)`: every record but the
//! last is exactly `rs` bytes, the last at most `rs`, and only the last carries the delimiter `0x02`.

use aes_gcm::aead::{AeadInOut, KeyInit, Nonce, Tag};
use aes_gcm::Aes128Gcm;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use super::HttpBodyDecryptError;

/// The record size [`HttpBodyEncryptionKey::encrypt`](super::HttpBodyEncryptionKey::encrypt) uses:
/// a record carries up to 4079 bytes of data.
pub const DEFAULT_RECORD_SIZE: u32 = 4096;

pub const SALT_LEN: usize = 16;
const CONTENT_KEY_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

/// The salt, the record size and the length of the key id.
const HEADER_LEN: usize = SALT_LEN + 4 + 1;

/// What a record adds to the data it carries: the tag and the delimiter.
const RECORD_OVERHEAD: usize = TAG_LEN + 1;

const DELIMITER: u8 = 0x01;
const LAST_DELIMITER: u8 = 0x02;

/// The `info` of HKDF-Expand; HKDF appends the `0x01` of its first block itself.
const CONTENT_KEY_INFO: &[u8] = b"Content-Encoding: aes128gcm\0";
const NONCE_INFO: &[u8] = b"Content-Encoding: nonce\0";

/// A fresh salt from the OS random source.
///
/// Panics if the OS random source gives nothing: a body is never encrypted under a salt that is
/// not fresh and random.
pub fn random_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).expect("the OS random source gives no salt");
    salt
}

/// `plain`, encrypted with `ikm` under `salt`, in records of `record_size` bytes, with `key_id` in
/// the header. No padding. A salt is never to be used twice with one key: the caller makes a fresh
/// one for every body.
///
/// Panics if `record_size` leaves no room for data (`<= 17`) or `key_id` is over 255 bytes: both
/// come from this crate, never from a body.
pub fn encrypt_with(
    ikm: &[u8],
    salt: &[u8; SALT_LEN],
    record_size: u32,
    key_id: &[u8],
    plain: &[u8],
) -> Vec<u8> {
    assert!(
        record_size as usize > RECORD_OVERHEAD,
        "a record of {record_size} bytes carries no data"
    );
    let key_id_len = u8::try_from(key_id.len()).expect("a key id is at most 255 bytes");

    let chunk_len = record_size as usize - RECORD_OVERHEAD;

    // Empty data is still a body: one last record, holding nothing but its delimiter.
    let records = plain.len().div_ceil(chunk_len).max(1);

    let keys = RecordKeys::derive(ikm, salt);

    let mut body = Vec::with_capacity(
        HEADER_LEN + key_id.len() + plain.len() + records * RECORD_OVERHEAD,
    );

    body.extend_from_slice(salt);
    body.extend_from_slice(&record_size.to_be_bytes());
    body.push(key_id_len);
    body.extend_from_slice(key_id);

    let mut chunks = plain.chunks(chunk_len);

    for sequence in 0..records {
        let record_start = body.len();

        body.extend_from_slice(chunks.next().unwrap_or_default());
        body.push(if sequence + 1 == records {
            LAST_DELIMITER
        } else {
            DELIMITER
        });

        // Sealed in place, in the body itself: the plain text never stays behind in it.
        let tag = keys
            .cipher
            .encrypt_inout_detached(
                &keys.nonce(sequence as u64),
                &[],
                (&mut body[record_start..]).into(),
            )
            .expect("AES-128-GCM seals a record of any size up to rs");

        body.extend_from_slice(&tag);
    }

    body
}

/// The body, decrypted with `ikm`. All of it or nothing: a record that fails its tag, a body cut
/// short, or anything after the last record refuses the whole body.
pub fn decrypt(ikm: &[u8], body: &[u8]) -> Result<Vec<u8>, HttpBodyDecryptError> {
    let header = Header::read(body).ok_or(HttpBodyDecryptError::NotEncrypted)?;

    let all_records = &body[header.len..];

    if all_records.is_empty() {
        return Err(HttpBodyDecryptError::CutShort);
    }

    let keys = RecordKeys::derive(ikm, header.salt);

    // The plain text is secret too. Until the whole body has opened it stays in buffers that are
    // wiped when they drop, so a body refused halfway leaves none of it behind.
    let mut plain = Zeroizing::new(Vec::new());
    let mut record_buffer = Zeroizing::new(Vec::new());

    let mut records = all_records;
    let mut sequence = 0u64;

    loop {
        // `rs` comes from the body, so it is only ever a bound: the records are split by it, and
        // nothing is allocated by it.
        let (record, rest) = records.split_at(records.len().min(header.record_size));

        if record.len() < RECORD_OVERHEAD {
            return Err(HttpBodyDecryptError::WrongKeyOrDamaged);
        }

        let (ciphertext, tag) = record.split_at(record.len() - TAG_LEN);
        let tag: &Tag<Aes128Gcm> = tag.try_into().expect("the tag is 16 bytes");

        // Every record but the last is the same size, and the last is no bigger: after the first
        // record this buffer never grows, so it never leaves a copy of plain text behind.
        record_buffer.clear();
        record_buffer.extend_from_slice(ciphertext);

        keys.cipher
            .decrypt_inout_detached(
                &keys.nonce(sequence),
                &[],
                record_buffer.as_mut_slice().into(),
                tag,
            )
            .map_err(|_| HttpBodyDecryptError::WrongKeyOrDamaged)?;

        // The padding is zero bytes after the delimiter.
        let Some(delimiter_at) = record_buffer.iter().rposition(|byte| *byte != 0) else {
            return Err(HttpBodyDecryptError::Malformed);
        };

        let is_last = rest.is_empty();

        match (record_buffer[delimiter_at], is_last) {
            (LAST_DELIMITER, true) | (DELIMITER, false) => {}
            (DELIMITER, true) => return Err(HttpBodyDecryptError::CutShort),
            _ => return Err(HttpBodyDecryptError::Malformed),
        }

        if sequence == 0 {
            // Reserved once the first record has proved the key, so a wrong key costs no
            // allocation the size of the body. The plain text is shorter than the records, so
            // `plain` never reallocates, and never leaves a copy of itself behind either.
            plain.reserve_exact(all_records.len());
        }

        plain.extend_from_slice(&record_buffer[..delimiter_at]);

        if is_last {
            // Moves the buffer out, it does not copy it: the empty `Vec` left behind is what drops.
            return Ok(std::mem::take(&mut *plain));
        }

        records = rest;
        sequence += 1;
    }
}

struct Header<'b> {
    salt: &'b [u8; SALT_LEN],
    record_size: usize,
    /// The header with its key id: where the records start.
    len: usize,
}

impl<'b> Header<'b> {
    fn read(body: &'b [u8]) -> Option<Self> {
        let salt: &[u8; SALT_LEN] = body.get(..SALT_LEN)?.try_into().ok()?;

        let record_size =
            u32::from_be_bytes(body.get(SALT_LEN..SALT_LEN + 4)?.try_into().ok()?) as usize;

        // A record has to hold its tag and its delimiter, and some data besides.
        if record_size <= RECORD_OVERHEAD {
            return None;
        }

        let len = HEADER_LEN + *body.get(HEADER_LEN - 1)? as usize;

        if body.len() < len {
            return None;
        }

        Some(Self {
            salt,
            record_size,
            len,
        })
    }
}

struct RecordKeys {
    /// AES-128-GCM keyed with the CEK. With aes-gcm's `zeroize` its key schedule is wiped on drop.
    cipher: Aes128Gcm,
    nonce_base: [u8; NONCE_LEN],
}

impl RecordKeys {
    /// `CEK` and `NONCE_BASE` of RFC 8188: HKDF-Extract with the salt, HKDF-Expand with the infos.
    fn derive(ikm: &[u8], salt: &[u8; SALT_LEN]) -> Self {
        let hkdf = Hkdf::<Sha256>::new(Some(salt), ikm);

        let mut content_key = Zeroizing::new([0u8; CONTENT_KEY_LEN]);
        hkdf.expand(CONTENT_KEY_INFO, &mut content_key[..])
            .expect("16 bytes is a length HKDF-SHA-256 gives");

        let mut nonce_base = [0u8; NONCE_LEN];
        hkdf.expand(NONCE_INFO, &mut nonce_base)
            .expect("12 bytes is a length HKDF-SHA-256 gives");

        let cipher =
            Aes128Gcm::new_from_slice(&content_key[..]).expect("16 bytes is an AES-128 key");

        Self { cipher, nonce_base }
    }

    /// The nonce of record `sequence`: `NONCE_BASE` XOR the sequence number as a 12-byte
    /// big-endian number. A record is at least 18 bytes, so the number always fits in its last 8
    /// bytes, and the first 4 stay as they are.
    fn nonce(&self, sequence: u64) -> Nonce<Aes128Gcm> {
        let mut nonce = self.nonce_base;

        for (byte, sequence_byte) in nonce[NONCE_LEN - 8..]
            .iter_mut()
            .zip(sequence.to_be_bytes())
        {
            *byte ^= sequence_byte;
        }

        nonce.into()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_vectors::*;
    use super::*;

    use HttpBodyDecryptError::*;

    #[test]
    fn the_vectors_decrypt_to_their_plain_text() {
        assert_eq!(
            decrypt(&base64url(RFC_3_1_KEY), &base64url(RFC_3_1_BODY)).unwrap(),
            WALRUS
        );
        assert_eq!(
            decrypt(&base64url(RFC_3_2_KEY), &base64url(RFC_3_2_BODY)).unwrap(),
            WALRUS
        );
        assert_eq!(
            decrypt(&base64(OUR_KEY), &base64(OUR_BODY)).unwrap(),
            OUR_PLAIN
        );
    }

    /// RFC 8188, 3.2 is read for what its header says, not only for its plain text.
    #[test]
    fn the_rfc_3_2_header_says_rs_25_and_key_id_a1() {
        let body = base64url(RFC_3_2_BODY);
        let header = Header::read(&body).unwrap();

        assert_eq!(header.record_size, 25);
        assert_eq!(&body[HEADER_LEN..header.len], b"a1");
        // Two records of exactly 25 bytes: the second is padded up to rs.
        assert_eq!(body.len() - header.len, 2 * 25);
    }

    #[test]
    fn the_vectors_are_encrypted_byte_for_byte() {
        let body = base64url(RFC_3_1_BODY);
        assert_eq!(
            encrypt_with(&base64url(RFC_3_1_KEY), &salt_of(&body), 4096, b"", WALRUS),
            body
        );

        let body = base64(OUR_BODY);
        assert_eq!(
            encrypt_with(&base64(OUR_KEY), &salt_of(&body), 4096, b"", OUR_PLAIN),
            body
        );
    }

    #[test]
    fn another_key_does_not_open_a_body() {
        assert_eq!(
            decrypt(&base64(OUR_KEY), &base64url(RFC_3_1_BODY)),
            Err(WrongKeyOrDamaged)
        );
        assert_eq!(
            decrypt(&base64url(RFC_3_1_KEY), &base64(OUR_BODY)),
            Err(WrongKeyOrDamaged)
        );
        assert_eq!(
            decrypt(&[0x55; 32], &base64(OUR_BODY)),
            Err(WrongKeyOrDamaged)
        );
    }

    /// The bytes of `rs` (16 to 19) are left out on purpose: RFC 8188 authenticates the records,
    /// not the header, and `rs` only says where records split. A body of one record opens under
    /// any `rs` at least as long as that record (`a_record_size_bigger_than_the_body_is_a_bound`).
    #[test]
    fn a_body_with_one_byte_changed_does_not_open() {
        let key = base64(OUR_KEY);
        let body = base64(OUR_BODY);

        // The salt, the key id length, the first record byte, the middle, the tag at the end.
        for index in [0, 15, 20, 21, body.len() / 2, body.len() - 1] {
            let mut damaged = body.clone();
            damaged[index] ^= 0x01;
            assert!(decrypt(&key, &damaged).is_err(), "byte {index}");
        }

        // A body of several records, damaged in each of them.
        let body = encrypt_with(&key, &[9; SALT_LEN], 25, b"", &data_of_len(30));
        for index in [HEADER_LEN, HEADER_LEN + 24, HEADER_LEN + 25, body.len() - 1] {
            let mut damaged = body.clone();
            damaged[index] ^= 0x80;
            assert_eq!(decrypt(&key, &damaged), Err(WrongKeyOrDamaged), "byte {index}");
        }
    }

    #[test]
    fn a_body_cut_short_does_not_open() {
        let key = base64(OUR_KEY);
        let body = base64(OUR_BODY);

        for len in [0, 10, HEADER_LEN - 1] {
            assert_eq!(decrypt(&key, &body[..len]), Err(NotEncrypted), "{len}");
        }
        assert_eq!(decrypt(&key, &body[..HEADER_LEN]), Err(CutShort));
        assert_eq!(decrypt(&key, &body[..HEADER_LEN + 5]), Err(WrongKeyOrDamaged));
        assert_eq!(decrypt(&key, &body[..body.len() - 1]), Err(WrongKeyOrDamaged));
    }

    /// A body of several records that lost its last one ends with a record that says more is
    /// coming.
    #[test]
    fn a_body_without_its_last_record_is_cut_short() {
        let key = base64(OUR_KEY);
        let body = encrypt_with(&key, &[7; SALT_LEN], 25, b"", &data_of_len(30));

        // 30 bytes in records of 8: four records, the last one shorter.
        assert_eq!(body.len(), HEADER_LEN + 3 * 25 + (6 + RECORD_OVERHEAD));

        for records in 1..=3 {
            let cut = &body[..HEADER_LEN + records * 25];
            assert_eq!(decrypt(&key, cut), Err(CutShort), "{records} records");
        }
    }

    #[test]
    fn data_after_the_last_record_is_malformed() {
        let key = base64(OUR_KEY);

        // 16 bytes in records of 8: two full records, the second one last.
        let mut body = encrypt_with(&key, &[5; SALT_LEN], 25, b"", &data_of_len(16));
        assert_eq!(body.len(), HEADER_LEN + 2 * 25);

        body.push(0);
        assert_eq!(decrypt(&key, &body), Err(Malformed));
    }

    #[test]
    fn a_record_without_a_delimiter_is_malformed() {
        let key = base64(OUR_KEY);
        let salt = [4; SALT_LEN];
        let keys = RecordKeys::derive(&key, &salt);

        // A record of nothing but zeros: all padding, no delimiter.
        let mut body = salt.to_vec();
        body.extend_from_slice(&4096u32.to_be_bytes());
        body.push(0);

        let mut record = vec![0u8; 5];
        let tag = keys
            .cipher
            .encrypt_inout_detached(&keys.nonce(0), &[], record.as_mut_slice().into())
            .unwrap();
        body.extend_from_slice(&record);
        body.extend_from_slice(&tag);

        assert_eq!(decrypt(&key, &body), Err(Malformed));
    }

    #[test]
    fn what_is_encrypted_opens_whatever_its_length() {
        let key = base64(OUR_KEY);

        for len in [0, 1, 7, 8, 9, 16, 17, 100, 4079, 4080, 10_000] {
            let plain = data_of_len(len);

            for record_size in [18, 25, 4096] {
                for key_id in [&b""[..], b"a1"] {
                    let body = encrypt_with(&key, &[3; SALT_LEN], record_size, key_id, &plain);
                    assert_eq!(
                        decrypt(&key, &body).unwrap(),
                        plain,
                        "{len} in {record_size}, key id {key_id:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn empty_data_is_one_record_holding_only_its_delimiter() {
        let key = base64(OUR_KEY);
        let body = encrypt_with(&key, &[1; SALT_LEN], 4096, b"", b"");

        assert_eq!(body.len(), HEADER_LEN + RECORD_OVERHEAD);
        assert_eq!(decrypt(&key, &body).unwrap(), b"");
    }

    #[test]
    fn a_record_size_of_17_or_less_is_not_encrypted() {
        let key = base64(OUR_KEY);
        let mut body = encrypt_with(&key, &[2; SALT_LEN], 18, b"", b"abc");
        assert_eq!(decrypt(&key, &body).unwrap(), b"abc");

        for record_size in [0u32, 1, 16, 17] {
            body[SALT_LEN..SALT_LEN + 4].copy_from_slice(&record_size.to_be_bytes());
            assert_eq!(decrypt(&key, &body), Err(NotEncrypted), "rs {record_size}");
        }
    }

    /// `rs` is untrusted: a body may claim records of 4 GiB, and is still read by what it holds.
    #[test]
    fn a_record_size_bigger_than_the_body_is_a_bound() {
        let key = base64(OUR_KEY);
        let mut body = base64(OUR_BODY);

        body[SALT_LEN..SALT_LEN + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(decrypt(&key, &body).unwrap(), OUR_PLAIN);
    }

    /// What is not encrypted is told from what is encrypted with another key only as far as the
    /// header goes: either way it does not open.
    #[test]
    fn plain_data_does_not_open() {
        let key = base64(OUR_KEY);

        for plain in [
            &b""[..],
            b"hosts: {}\n",
            b"ip_white_lists:\n  office:\n  - 10.0.0.1\n  - 10.0.0.2\n  - 10.0.0.3\n",
            b"-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
        ] {
            assert!(decrypt(&key, plain).is_err(), "{plain:?}");
        }
    }
}
