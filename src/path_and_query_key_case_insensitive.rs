use crate::PathAndQueryKey;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathAndQueryKeyCaseInsensitive(String);

impl PathAndQueryKeyCaseInsensitive {
    pub fn from_path_and_query(path_and_query: &str) -> Self {
        // Shares the single normalizer with PathAndQueryKey, folding only the path.
        Self(crate::path_and_query_key::build_key(path_and_query, true))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn has_query(&self) -> bool {
        self.0.contains('?')
    }
}

impl std::fmt::Display for PathAndQueryKeyCaseInsensitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<PathAndQueryKey> for PathAndQueryKeyCaseInsensitive {
    fn from(src: PathAndQueryKey) -> Self {
        // Re-parse through the same normalizer so the conversion route and the direct
        // constructor always agree (previously this lowercased the whole string,
        // including the query, while from_path_and_query lowercased only the path).
        Self::from_path_and_query(src.as_str())
    }
}

#[cfg(test)]
mod test {

    use crate::{PathAndQueryKey, PathAndQueryKeyCaseInsensitive};

    #[test]
    fn test_basic() {
        let path1 = PathAndQueryKeyCaseInsensitive::from_path_and_query(
            "/path/to/some/where?name=John&age=20",
        );

        let path2 = PathAndQueryKeyCaseInsensitive::from_path_and_query(
            "/path/to/Some/where?age=20&name=John",
        );

        assert_eq!(path1.as_str(), path2.as_str());

        assert_eq!(path1.has_query(), true);
    }

    #[test]
    fn test_basic_2() {
        let path1 = PathAndQueryKeyCaseInsensitive::from_path_and_query("/path/to/some/where");

        let path2 = PathAndQueryKeyCaseInsensitive::from_path_and_query("/path/to/Some/where");

        assert_eq!(path1.as_str(), path2.as_str());

        assert_eq!(path1.has_query(), false);
    }

    #[test]
    fn test_basic_3() {
        let path1 = PathAndQueryKeyCaseInsensitive::from_path_and_query(
            "/path/to/some/where?name=John&age=20&married",
        );

        let path2 = PathAndQueryKeyCaseInsensitive::from_path_and_query(
            "/path/to/Some/where?married&age=20&name=John",
        );

        assert_eq!(path1.as_str(), path2.as_str());

        assert_eq!(path1.has_query(), true);
    }

    #[test]
    fn test_from_route_matches_direct_constructor() {
        // Regression: the two construction routes used to disagree because the Into
        // impl lowercased the whole string (query included).
        let src = "/Path/To/Where?Name=John";
        let via_from: PathAndQueryKeyCaseInsensitive =
            PathAndQueryKey::from_path_and_query(src).into();
        let direct = PathAndQueryKeyCaseInsensitive::from_path_and_query(src);
        assert_eq!(via_from.as_str(), direct.as_str());
    }
}
