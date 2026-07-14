use super::UrlDecodeError;

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

            let result = decode_escaped(esc_string_as_slice)?;

            return Ok(Some(result));
        }

        Ok(None)
    }
}

pub fn decode_escaped(encoded: &[u8]) -> Result<u8, UrlDecodeError> {
    let b0 = decode_hex_symbol(encoded[1])?;
    let b1 = decode_hex_symbol(encoded[2])?;

    let symbol = b1 + b0 * 16;

    Ok(symbol)
}

const ZERO: u8 = b'0';
const A_LOWER: u8 = b'a' - 10;
const A_CAPITAL: u8 = b'A' - 10;

fn decode_hex_symbol(c: u8) -> Result<u8, UrlDecodeError> {
    if c.is_ascii_digit() {
        return Ok(c - ZERO);
    }

    if (b'a'..=b'f').contains(&c) {
        return Ok(c - A_LOWER);
    }

    if (b'A'..=b'F').contains(&c) {
        return Ok(c - A_CAPITAL);
    }

    Err(UrlDecodeError {
        msg: format!("Invalid escape char '{}'", c as char),
    })
}
