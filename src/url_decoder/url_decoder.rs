use crate::url_decoder::escaped_state::EscapedState;

use super::{normal_state::NormalState, url_decode_state::UrlDecodeState, UrlDecodeError};

pub struct UrlDecoder<'s> {
    src: &'s [u8],
    pos: usize,
    state: UrlDecodeState,
}

impl<'s> UrlDecoder<'s> {
    pub fn new(src: &'s str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
            state: UrlDecodeState::Normal(NormalState {}),
        }
    }

    pub fn get_next(&mut self) -> Result<Option<u8>, UrlDecodeError> {
        loop {
            if self.pos >= self.src.len() {
                return Ok(None);
            }

            let next_char = self.src[self.pos];

            self.pos += 1;

            match &mut self.state {
                UrlDecodeState::Normal(state) => {
                    if state.get_next(next_char) {
                        return Ok(Some(next_char));
                    }

                    self.state = UrlDecodeState::Escaped(EscapedState::new(next_char));
                }
                UrlDecodeState::Escaped(state) => {
                    let next_result = state.get_next(next_char)?;

                    if let Some(next_symbol) = next_result {
                        self.state = UrlDecodeState::Normal(NormalState {});
                        return Ok(Some(next_symbol));
                    }
                }
            }
        }
    }
}
