use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    static ref URL_DECODE_SYMBOLS_2: HashMap<u8, u8> = [
        (b'3', b'#'),
        (b'4', b'$'),
        (b'5', b'%'),
        (b'6', b'&'),
        (b'7', b'\''),
        (b'8', b'('),
        (b'9', b')'),
        (b'A', b'*'),
        (b'a', b'*'),
        (b'B', b'+'),
        (b'b', b'+'),
        (b'C', b','),
        (b'c', b','),
        (b'F', b'/'),
        (b'f', b'/'),
    ]
    .iter()
    .copied()
    .collect();
}

lazy_static! {
    static ref URL_DECODE_SYMBOLS_3: HashMap<u8, u8> = [
        (b'A', b':'),
        (b'a', b':'),
        (b'B', b';'),
        (b'b', b';'),
        (b'D', b'='),
        (b'd', b'='),
        (b'F', b'?'),
        (b'f', b'?'),
//        ('@', "%40"),
//        ('[', "%5B"),
//        (']', "%5D"),
    ]
    .iter()
    .copied()
    .collect();
}
