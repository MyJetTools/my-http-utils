/// How hard [`HttpRequestBody::compress_with_level`](super::HttpRequestBody) works at a body: the
/// time it takes to compress traded for the size of what goes on the wire. The type is there without
/// the `compression` feature as well, so a transport can name it in its own api either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionLevel {
    /// The least time, the least gain — gzip level 1. For a big body sent often, when the CPU is
    /// worth more than the bytes.
    Fast,
    /// The balance gzip itself defaults to — level 6. What `compress` uses.
    #[default]
    Default,
    /// The smallest body, the most time — gzip level 9. For a slow or metered link.
    Best,
}

/// A body shorter than this only gets bigger gzipped (the gzip header and trailer alone are 18
/// bytes), so it is left as it is.
#[cfg(feature = "compression")]
pub(crate) const MIN_LEN_TO_COMPRESS: usize = 64;

#[cfg(feature = "compression")]
impl CompressionLevel {
    fn to_gzip(self) -> flate2::Compression {
        match self {
            Self::Fast => flate2::Compression::fast(),
            Self::Default => flate2::Compression::default(),
            Self::Best => flate2::Compression::best(),
        }
    }
}

/// `body` gzipped at `level`.
#[cfg(feature = "compression")]
pub(crate) fn gzip(body: &[u8], level: CompressionLevel) -> Vec<u8> {
    use std::io::Write;

    // The encoder writes into a Vec, which does not fail: the io::Result is only the shape of its
    // api.
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), level.to_gzip());
    encoder
        .write_all(body)
        .expect("gzip into a Vec does not fail");
    encoder.finish().expect("gzip into a Vec does not fail")
}

#[cfg(all(test, feature = "compression"))]
pub(crate) mod tests {
    use std::io::Read;

    use super::*;

    pub const LEVELS: [CompressionLevel; 3] = [
        CompressionLevel::Fast,
        CompressionLevel::Default,
        CompressionLevel::Best,
    ];

    pub fn compressible_body() -> Vec<u8> {
        (0..2000)
            .map(|i| {
                format!(
                    "{{\"id\":{},\"name\":\"user-{}\",\"active\":true}},",
                    i,
                    i % 37
                )
            })
            .collect::<String>()
            .into_bytes()
    }

    pub fn gunzip(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        flate2::read::GzDecoder::new(data)
            .read_to_end(&mut result)
            .unwrap();
        result
    }

    #[test]
    fn every_level_gives_a_gzip_of_the_same_body() {
        let body = compressible_body();

        for level in LEVELS {
            let compressed = gzip(&body, level);
            assert!(compressed.len() < body.len(), "{level:?}");
            assert_eq!(gunzip(&compressed), body, "{level:?}");
        }
    }

    #[test]
    fn best_gives_a_smaller_body_than_fast() {
        let body = compressible_body();

        let fast = gzip(&body, CompressionLevel::Fast);
        let best = gzip(&body, CompressionLevel::Best);

        assert!(
            best.len() < fast.len(),
            "best {} vs fast {}",
            best.len(),
            fast.len()
        );
    }

    #[test]
    fn the_default_level_is_default() {
        assert_eq!(CompressionLevel::default(), CompressionLevel::Default);
    }
}
