use super::path_segment::PathSegment;

pub enum GetPathValueResult<'s> {
    NoKeyInTheRoute,
    NoValue,
    Value(&'s str),
}

impl<'s> GetPathValueResult<'s> {
    pub fn is_no_key(&'s self) -> bool {
        match self {
            GetPathValueResult::NoKeyInTheRoute => true,
            GetPathValueResult::NoValue => false,
            GetPathValueResult::Value(_) => false,
        }
    }

    pub fn is_no_value(&'s self) -> bool {
        match self {
            GetPathValueResult::NoKeyInTheRoute => false,
            GetPathValueResult::NoValue => true,
            GetPathValueResult::Value(_) => false,
        }
    }

    pub fn unwrap(&'s self) -> &'s str {
        match self {
            GetPathValueResult::NoKeyInTheRoute => panic!("No key in the search result"),
            GetPathValueResult::NoValue => panic!("No value in the search result"),
            GetPathValueResult::Value(value) => value,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PathSegments {
    pub path: String,
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

        mark_last(&mut segments);

        Self {
            segments,
            keys_amount,
            path: path.to_string(),
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

            if let Some(the_path_segment) = self.segments.get(index) {
                match the_path_segment {
                    PathSegment::Path(the_path_segment) => {
                        if the_path_segment != &path_segment.to_lowercase() {
                            return false;
                        }
                    }
                    PathSegment::Key { path_key: _, last } => {
                        if *last {
                            break;
                        }
                    }
                }
            } else {
                return false;
            }

            index += 1;
        }

        return true;
    }

    pub fn get_value<'s>(&self, path: &'s str, key: &str) -> GetPathValueResult<'s> {
        if self.keys_amount == 0 {
            return GetPathValueResult::NoKeyInTheRoute;
        }

        let mut index = 0;

        for path_segment in path.split('/') {
            if path_segment.len() == 0 {
                continue;
            }

            if let Some(segment) = self.segments.get(index) {
                if let PathSegment::Key { path_key, last: _ } = segment {
                    if path_key == key {
                        return GetPathValueResult::Value(path_segment);
                    }
                }
            } else {
                return GetPathValueResult::NoKeyInTheRoute;
            }

            index += 1;
        }

        return GetPathValueResult::NoValue;
    }
}

fn mark_last(segments: &mut Vec<PathSegment>) {
    let mut index = segments.len() - 1;
    loop {
        match segments.get_mut(index).unwrap() {
            PathSegment::Path(_) => break,
            PathSegment::Key { path_key: _, last } => {
                *last = true;
            }
        }

        if index == 0 {
            break;
        }

        index -= 1;
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

    #[test]
    fn test_last_marker() {
        let result = PathSegments::new("/Segment1/{Key1}/Segment2/{Key2}");

        let value = result.segments.get(1).unwrap();
        if let PathSegment::Key { path_key: _, last } = value {
            assert_eq!(false, *last);
        } else {
            panic!("Should not be gere")
        }

        let value = result.segments.get(3).unwrap();
        if let PathSegment::Key { path_key: _, last } = value {
            assert_eq!(true, *last);
        } else {
            panic!("Should not be gere")
        }
    }

    #[test]
    fn test_is_my_path_with_last_key() {
        let path_segments = PathSegments::new("/Segment1/Segment2/{Key1}");

        assert_eq!(true, path_segments.is_my_path("/Segment1/Segment2"));

        let value = path_segments.get_value("/Segment1/Segment2", "Key1");

        assert_eq!(true, value.is_no_value());
    }
}
