pub struct PathAndQueryReader<'s> {
    pub path: &'s str,
    pub query: Option<&'s str>,
}

impl<'s> PathAndQueryReader<'s> {
    pub fn new(path_and_query: &'s str) -> Self {
        if path_and_query.is_empty() {
            return Self {
                path: "/",
                query: None,
            };
        }

        let mut split = path_and_query.splitn(2, '?');
        let left = split.next().unwrap();
        let right = split.next();

        // Normalize the path: empty -> "/", and strip a single trailing slash — but
        // never collapse the root "/" itself into an empty string.
        let path = if left.is_empty() {
            "/"
        } else if left.len() > 1 {
            left.strip_suffix('/').unwrap_or(left)
        } else {
            left
        };

        Self { path, query: right }
    }

    pub fn is_my_path(&self, path: &str) -> bool {
        let path = if path.is_empty() { "/" } else { path };

        // Compare trailing-slash-insensitively on both sides.
        let path = if path.len() > 1 {
            path.strip_suffix('/').unwrap_or(path)
        } else {
            path
        };

        if path.starts_with('/') {
            return self.path.eq_ignore_ascii_case(path);
        }

        // Argument has no leading '/', so compare against our path without its own.
        let self_path = self.path.strip_prefix('/').unwrap_or(self.path);
        self_path.eq_ignore_ascii_case(path)
    }
}

#[cfg(test)]
mod tests {
    use crate::PathAndQueryReader;

    #[test]
    fn test_my_path() {
        let src = PathAndQueryReader::new("/my-path?param=1");

        assert!(src.is_my_path("/my-path"));
        assert!(src.is_my_path("/my-Path"));
        assert!(src.is_my_path("my-Path"));
        assert!(!src.is_my_path("my-Path-2"));

        assert_eq!(src.path, "/my-path");
        assert_eq!(src.query, Some("param=1"));
    }

    #[test]
    fn test_root_path() {
        // Regression: "/" used to become "" and is_my_path used to panic.
        let src = PathAndQueryReader::new("/");
        assert_eq!(src.path, "/");
        assert!(src.is_my_path("/"));
        assert!(!src.is_my_path("other"));
    }

    #[test]
    fn test_empty_input() {
        let src = PathAndQueryReader::new("");
        assert_eq!(src.path, "/");
        assert_eq!(src.query, None);
        assert!(src.is_my_path("/"));
    }

    #[test]
    fn test_query_with_second_question_mark() {
        // Regression: split('?') truncated the query at the second '?'.
        let src = PathAndQueryReader::new("/p?redirect=/a?b=c");
        assert_eq!(src.path, "/p");
        assert_eq!(src.query, Some("redirect=/a?b=c"));
    }

    #[test]
    fn test_trailing_slash_symmetry() {
        let src = PathAndQueryReader::new("/my-path/");
        assert_eq!(src.path, "/my-path");
        assert!(src.is_my_path("/my-path"));
        assert!(src.is_my_path("/my-path/"));
    }
}
