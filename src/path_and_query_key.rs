use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathAndQueryKey(String);

impl PathAndQueryKey {
    pub fn from_path_and_query(path_and_query: &str) -> Self {
        Self(build_key(path_and_query, false))
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

impl std::fmt::Display for PathAndQueryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Builds a canonical, order-insensitive key out of a path+query string.
///
/// * The query is split at the first `?` only (`splitn`), so a value that itself
///   contains `?` is not truncated.
/// * Each `key=value` pair is split at the first `=` only, so values containing `=`
///   (e.g. base64 padding) survive intact.
/// * Query parameters are grouped by key and the values within a key are sorted, so
///   duplicate keys are preserved and reordering the query never changes the key.
/// * When `lowercase_path` is set, only the path is case-folded (query is left as-is).
pub(crate) fn build_key(path_and_query: &str, lowercase_path: bool) -> String {
    let mut iterator = path_and_query.splitn(2, '?');

    let path = iterator.next().unwrap_or("/");
    let path = if path.is_empty() { "/" } else { path };

    let path_out = if lowercase_path {
        path.to_lowercase()
    } else {
        path.to_string()
    };

    let Some(query) = iterator.next() else {
        return path_out;
    };

    let mut query_builder: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();

    for key_value in query.split('&') {
        let mut key_value_iterator = key_value.splitn(2, '=');

        let key = key_value_iterator.next().unwrap_or("").to_string();
        let value = key_value_iterator.next().map(|v| v.to_string());

        query_builder.entry(key).or_default().push(value);
    }

    let mut query_result = String::new();

    for (key, mut values) in query_builder {
        values.sort();
        for value in values {
            if !query_result.is_empty() {
                query_result.push('&');
            }
            query_result.push_str(&key);

            if let Some(value) = value {
                query_result.push('=');
                query_result.push_str(&value);
            }
        }
    }

    format!("{}?{}", path_out, query_result)
}

#[cfg(test)]
mod test {

    use crate::PathAndQueryKey;

    #[test]
    fn test_basic() {
        let path1 = PathAndQueryKey::from_path_and_query("/path/to/Some/where?name=John&age=20");

        let path2 = PathAndQueryKey::from_path_and_query("/path/to/Some/where?age=20&name=John");

        assert_eq!(path1.as_str(), path2.as_str());
        assert_eq!(path1.as_str(), "/path/to/Some/where?age=20&name=John");

        assert_eq!(path1.has_query(), true);
    }

    #[test]
    fn test_basic_2() {
        let path1 = PathAndQueryKey::from_path_and_query("/path/to/Some/where");

        let path2 = PathAndQueryKey::from_path_and_query("/path/to/Some/where");

        assert_eq!(path1.as_str(), path2.as_str());
        assert_eq!(path1.as_str(), "/path/to/Some/where");

        assert_eq!(path1.has_query(), false);
    }

    #[test]
    fn test_basic_3() {
        let path1 =
            PathAndQueryKey::from_path_and_query("/path/to/Some/where?name=John&age=20&married");

        let path2 =
            PathAndQueryKey::from_path_and_query("/path/to/Some/where?married&age=20&name=John");

        assert_eq!(path1.as_str(), path2.as_str());
        assert_eq!(path1.as_str(), "/path/to/Some/where?age=20&married&name=John");

        assert_eq!(path1.has_query(), true);
    }

    #[test]
    fn test_value_with_equals_is_not_truncated() {
        let key = PathAndQueryKey::from_path_and_query("/p?token=YWJjZA==");
        assert_eq!(key.as_str(), "/p?token=YWJjZA==");
    }

    #[test]
    fn test_duplicate_keys_are_preserved_and_order_insensitive() {
        let a = PathAndQueryKey::from_path_and_query("/p?x=1&x=2");
        let b = PathAndQueryKey::from_path_and_query("/p?x=2&x=1");
        assert_eq!(a.as_str(), b.as_str());
        assert_eq!(a.as_str(), "/p?x=1&x=2");
    }

    #[test]
    fn test_value_with_question_mark_is_not_truncated() {
        let key = PathAndQueryKey::from_path_and_query("/p?redirect=/a?b=c");
        assert_eq!(key.as_str(), "/p?redirect=/a?b=c");
    }

    #[test]
    fn test_display_matches_as_str() {
        let key = PathAndQueryKey::from_path_and_query("/p?a=1");
        assert_eq!(key.to_string(), key.as_str());
    }
}
