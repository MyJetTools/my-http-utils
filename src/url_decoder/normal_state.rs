pub struct NormalState {}

const SWITCH_SYMBOL: u8 = b'%';

impl NormalState {
    pub fn get_next(&self, next: u8) -> bool {
        next != SWITCH_SYMBOL
    }
}
