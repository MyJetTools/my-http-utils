use std::collections::HashMap;

use super::UrlDecodeError;
use lazy_static::lazy_static;

pub struct EscapedState {
    buffer: [u8; 3],
    pos: usize,
}

impl EscapedState {
    pub fn new(first: u8) -> Self {
        Self {
            buffer: [first, 0u8, 0u8],
            pos: 1,
        }
    }

    pub fn get_next(&mut self, next_char: u8) -> Result<Option<u8>, UrlDecodeError> {
        self.buffer[self.pos] = next_char;
        self.pos += 1;

        if self.pos == 3 {
            let esc_string_as_slice = &self.buffer;
            let esc_string = std::str::from_utf8(esc_string_as_slice)?;

            let result = URL_DECODE_SYMBOLS.get(esc_string);

            if let Some(result) = result {
                return Ok(Some(*result));
            }

            return Err(UrlDecodeError {
                msg: format!("Invalid escape string {}", esc_string),
            });
        }

        return Ok(None);
    }
}

lazy_static! {
    static ref URL_DECODE_SYMBOLS: HashMap<&'static str, u8> = [
        ("%21", b'!'),
        ("%23", b'#'),
        ("%24", b'$'),
        ("%25", b'%'),
        ("%26", b'&'),
        ("%27", b'\''),
        ("%28", b'('),
        ("%29", b')'),
        ("%2A", b'*'),
        ("%2B", b'+'),
        ("%2C", b','),
        ("%2F", b'/'),
        ("%3A", b':'),
        ("%3B", b';'),
        ("%3D", b'='),
        ("%3F", b'?'),
        ("%40", b'@'),
        ("%5B", b'['),
        ("%5D", b']'),
    ]
    .iter()
    .copied()
    .collect();
}
