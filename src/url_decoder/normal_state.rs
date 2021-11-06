pub struct NormalState {}

const SWITCH_SYMBOL: u8 = '%' as u8;

impl NormalState {
    pub fn get_next(&self, next: u8) -> bool {
        return next != SWITCH_SYMBOL;
    }
}
