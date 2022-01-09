#[derive(Debug, Clone)]
pub enum PathSegment {
    Path(String),
    Key { path_key: String, last: bool },
}

impl PathSegment {
    pub fn new(path_segment: &str) -> Self {
        if path_segment.len() < 3 {
            return PathSegment::Path(path_segment.to_lowercase());
        }

        if path_segment.starts_with("{") && path_segment.ends_with("}") {
            return PathSegment::Key {
                path_key: path_segment[1..path_segment.len() - 1].to_string(),
                last: false,
            };
        }

        return PathSegment::Path(path_segment.to_lowercase());
    }

    pub fn is_key(&self) -> bool {
        match self {
            PathSegment::Path(_) => false,
            PathSegment::Key {
                path_key: _,
                last: _,
            } => true,
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_segment_as_path() {
        let result = PathSegment::new("Test");

        if let PathSegment::Path(path) = result {
            assert_eq!("test", path)
        } else {
            panic!("Should not be here")
        }
    }

    #[test]
    fn test_segment_as_key() {
        let result = PathSegment::new("{Test}");
        assert_eq!(true, result.is_key());
    }
}
