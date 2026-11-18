//! The test vectors of my-reverse-proxy's `docs/encrypted-service-data.md`: the first two are
//! RFC 8188's own (base64url, 16-byte keys), the third is that standard's (standard base64, a key of
//! 32 bytes). The salt of each is the first 16 bytes of its body.

use base64::Engine;

use super::aes128gcm::SALT_LEN;

pub fn base64url(text: &str) -> Vec<u8> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text)
        .unwrap()
}

pub fn base64(text: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .unwrap()
}

pub fn to_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn salt_of(body: &[u8]) -> [u8; SALT_LEN] {
    body[..SALT_LEN].try_into().unwrap()
}

/// `len` bytes of data that is not all one byte.
pub fn data_of_len(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index % 251) as u8).collect()
}

pub const WALRUS: &[u8] = b"I am the walrus";

/// RFC 8188, 3.1: one record, no key id.
pub const RFC_3_1_KEY: &str = "yqdlZ-tYemfogSmv7Ws5PQ";
pub const RFC_3_1_BODY: &str =
    "I1BsxtFttlv3u_Oo94xnmwAAEAAA-NAVub2qFgBEuQKRapoZu-IxkIva3MEB1PD-ly8Thjg";

/// RFC 8188, 3.2: records of 25 bytes, the key id `a1`, and padding in the last record.
pub const RFC_3_2_KEY: &str = "BO3ZVPxUlnLORbVGMpbT1Q";
pub const RFC_3_2_BODY: &str = "uNCkWiNYzKTnBN9ji3-qWAAAABkCYTHOG8chz_gnvgOqdGYovxyjuqRyJFjEDyoF1Fvkj6hQPdPHI51OEUKEpgz3SsLWIqS_uA";

/// The standard's own vector: the key is the bytes 0 to 31, in standard base64.
pub const OUR_KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
pub const OUR_BODY: &str = "oKGio6SlpqeoqaqrrK2urwAAEAAAd4lQdR25LBqmiaRlRbryiJTYDikv7OAMtydV1zSRPu4Fy70/BNOmc0q8RjUIqwznMD1/D9zqlMc=";
pub const OUR_PLAIN: &[u8] = b"ip_white_lists:\n  office:\n  - 10.0.0.1\n";
