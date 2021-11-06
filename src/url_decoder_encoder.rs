use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    static ref URL_ENCODE_SYMBOLS: HashMap<char, &'static str> = [
        (' ', "+"),
        ('#', "%23"),
        ('$', "%24"),
        ('%', "%25"),
        ('&', "%26"),
        ('\'', "%27"),
        ('(', "%28"),
        (')', "%29"),
        ('*', "%2A"),
        ('+', "%2B"),
        (',', "%2C"),
        ('/', "%2F"),
        (':', "%3A"),
        (';', "%3B"),
        ('=', "%3D"),
        ('?', "%3F"),
        ('@', "%40"),
        ('[', "%5B"),
        (']', "%5D"),
    ]
    .iter()
    .copied()
    .collect();
}
