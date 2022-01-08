use super::path_segment::PathSegment;

pub struct PathSegments {
    pub segments: Vec<PathSegment>,
    pub keys_amount: usize,
}

impl PathSegments {
    pub fn new(path: &str) -> Self {
        let mut segments = Vec::new();
        let mut keys_amount = 0;
        for path_segment in path.split('/') {
            if path_segment.len() == 0 {
                continue;
            }

            let path_segment = PathSegment::new(path_segment);

            if path_segment.is_key() {
                keys_amount += 1;
            }

            segments.push(path_segment);
        }

        Self {
            segments,
            keys_amount,
        }
    }

    pub fn is_my_path(&self, path: &str) -> bool {
        if self.segments.len() == 0 && path == "/" {
            return true;
        }

        let mut index = 0;

        for path_segment in path.split('/') {
            if path_segment.len() == 0 {
                continue;
            }

            if let Some(segment) = self.segments.get(index) {
                if let PathSegment::Path(segment) = segment {
                    if segment != &path_segment.to_lowercase() {
                        return false;
                    }
                }
            } else {
                return false;
            }

            index += 1;
        }

        return true;
    }

    pub fn get_value<'s>(&self, path: &'s str, key: &str) -> Option<&'s str> {
        if self.keys_amount == 0 {
            return None;
        }

        let mut index = 0;

        for path_segment in path.split('/') {
            if path_segment.len() == 0 {
                continue;
            }

            if let Some(segment) = self.segments.get(index) {
                if let PathSegment::Key(path_key) = segment {
                    if path_key == key {
                        return Some(path_segment);
                    }
                }
            } else {
                return None;
            }

            index += 1;
        }

        return None;
    }
}
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_is_my_path() {
        let path_segments = PathSegments::new("/Segment1/{Key1}/Segment2");

        assert_eq!(true, path_segments.is_my_path("/Segment1/MyValue/Segment2"));

        assert_eq!(
            "MyValue",
            path_segments
                .get_value("/Segment1/MyValue/Segment2", "Key1")
                .unwrap()
        );
    }

    #[test]
    fn test_is_not_my_path() {
        let path_segments = PathSegments::new("/Segment1/{Key1}/Segment2");

        assert_eq!(
            false,
            path_segments.is_my_path("/Segment2/MyValue/Segment2")
        );
    }
}
