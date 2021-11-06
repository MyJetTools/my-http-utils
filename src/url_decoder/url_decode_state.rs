use super::{escaped_state::EscapedState, normal_state::NormalState};

pub enum UrlDecodeState {
    Normal(NormalState),
    Escaped(EscapedState),
}
