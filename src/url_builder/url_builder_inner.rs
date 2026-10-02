use core::str;
use std::borrow::Cow;

use rust_extensions::remote_endpoint::{RemoteEndpoint, Scheme};

pub struct UrlBuilderInner {
    value: String,
    host_index: usize,
    port_index: usize,
    path_index: usize,
    query_index: usize,
}

impl UrlBuilderInner {
    pub fn new(host_port: &str) -> Self {
        let mut value = String::new();

        let host_index = if let Some(host_index) = host_index_after_scheme(host_port) {
            host_index
        } else {
            value.push_str("http://");
            7
        };
        value.push_str(host_port);

        let mut port_index = 0;
        let mut path_index = 0;
        let mut query_index = 0;

        // An IPv6 literal is written in brackets, and the colons inside them belong to
        // the address: only a colon after the closing bracket separates the port.
        let mut inside_brackets = value.as_bytes().get(host_index) == Some(&b'[');

        // Scan by byte index. The delimiters ':' '/' '?' are ASCII, and no ASCII
        // byte ever appears inside a multi-byte UTF-8 sequence, so byte offsets are
        // safe to feed back into slicing (unlike the previous char-count positions).
        //
        // The scan starts AT host_index: the first character after the scheme is a
        // delimiter like any other, so `http://:8080` has an empty host and a port.
        for (pos, b) in value.bytes().enumerate().skip(host_index) {
            match b {
                b']' => {
                    inside_brackets = false;
                }
                b':' => {
                    if path_index == 0 && !inside_brackets {
                        port_index = pos;
                    }
                }
                b'/' => {
                    if path_index == 0 {
                        path_index = pos;
                    }
                }
                b'?' => {
                    // path_index stays 0 when the query comes with no path, the same
                    // as after append_query_param on a path-less url.
                    query_index = pos;
                    break;
                }
                _ => {}
            }
        }

        Self {
            value,
            host_index,
            path_index,
            port_index,
            query_index,
        }
    }

    pub fn get_remote_endpoint<'s>(&'s self, default_port: Option<u16>) -> RemoteEndpoint<'s> {
        // Parse only scheme://host:port — RemoteEndpoint terminates the host at the
        // first '/', so feeding it a path/query-less prefix keeps host/port correct
        // even when a query was appended before any path.
        //
        // RemoteEndpoint refuses a scheme it does not know, while get_scheme() reads
        // such a scheme as http — so the endpoint is then made of host:port alone,
        // which has no '/' for a scheme to end at and therefore always parses.
        let mut result = RemoteEndpoint::try_parse(self.get_scheme_and_host())
            .or_else(|_| RemoteEndpoint::try_parse(self.get_host_port()))
            .unwrap_or_else(|_| unreachable!("host:port has no scheme to be refused"));

        if let Some(default_port) = default_port {
            result.set_default_port(default_port);
        }

        result
    }

    /// Makes the value — which has to end where the path ends — end with the '/' the
    /// next piece of the path goes after.
    fn push_path_separator(&mut self) {
        if self.path_index == 0 {
            // No path yet, so it starts here. The last character says nothing in this
            // case: for a url with no host it is the '/' of "://", not of a path.
            self.path_index = self.value.len();
            self.value.push('/');
        } else if !self.value.ends_with('/') {
            self.value.push('/');
        }
    }

    pub fn append_path_segment(&mut self, path: &str) {
        let segment = path.strip_prefix('/').unwrap_or(path);

        // If a query has already been appended, splice the segment in before '?'
        // instead of after it (which would make path_index > query_index and panic
        // on the backwards slice in get_path).
        let query = if self.query_index != 0 {
            Some(self.value.split_off(self.query_index))
        } else {
            None
        };

        self.push_path_separator();
        crate::url_encoder::encode_path_segment_and_copy(&mut self.value, segment);

        if let Some(query) = query {
            self.query_index = self.value.len();
            self.value.push_str(&query);
        }
    }

    pub fn append_query_param(&mut self, param: &str, value: Option<&str>) {
        if self.query_index == 0 {
            self.value.push('?');
            self.query_index = self.value.len() - 1;
        } else {
            self.value.push('&');
        }
        crate::encode_to_url_string_and_copy(&mut self.value, param);
        if let Some(value) = value {
            self.value.push('=');
            crate::encode_to_url_string_and_copy(&mut self.value, value);
        }
    }

    pub fn append_raw_ending(&mut self, raw_ending: &str) {
        let raw_ending = raw_ending.strip_prefix('/').unwrap_or(raw_ending);

        // After a query the ending lands in the query, as it always did. The path is
        // left alone: recording it here would put path_index after query_index, which
        // made get_path panic and get_host swallow the query.
        if self.query_index != 0 {
            if !self.value.ends_with('/') {
                self.value.push('/');
            }
            self.value.push_str(raw_ending);
            return;
        }

        // Only record the path start if there is no path yet; overwriting it would
        // make get_path/get_path_and_query drop a pre-existing base path while
        // to_string() still keeps it.
        self.push_path_separator();

        let ending_index = self.value.len();
        self.value.push_str(raw_ending);

        if let Some(index) = raw_ending.find('?') {
            self.query_index = ending_index + index;
        }
    }

    pub fn get_scheme(&self) -> Scheme {
        let index = self.value.find(":/");

        if index.is_none() {
            return Scheme::Http;
        }

        match Scheme::try_parse(&self.value[..index.unwrap()]) {
            Some(scheme) => scheme,
            None => Scheme::Http,
        }
    }

    pub fn get_host(&self) -> &str {
        if self.port_index > 0 {
            return &self.value[self.host_index..self.port_index];
        }

        if self.path_index > 0 {
            return &self.value[self.host_index..self.path_index];
        }

        if self.query_index > 0 {
            return &self.value[self.host_index..self.query_index];
        }

        &self.value[self.host_index..]
    }

    pub fn get_host_port(&self) -> &str {
        if self.get_scheme().is_unix_socket() {
            if self.query_index > 0 {
                return &self.value[self.host_index - 1..self.query_index];
            } else {
                return &self.value[self.host_index - 1..];
            }
        }

        if self.path_index > 0 {
            return &self.value[self.host_index..self.path_index];
        }

        if self.query_index > 0 {
            return &self.value[self.host_index..self.query_index];
        }

        &self.value[self.host_index..]
    }

    pub fn get_scheme_and_host(&self) -> &str {
        if self.get_scheme().is_unix_socket() {
            if self.query_index > 0 {
                return &self.value[..self.query_index];
            } else {
                return &self.value;
            }
        }

        if self.path_index > 0 {
            return &self.value[..self.path_index];
        }

        if self.query_index > 0 {
            return &self.value[..self.query_index];
        }

        &self.value
    }

    pub fn get_path_and_query(&self) -> Cow<'_, str> {
        if self.get_scheme().is_unix_socket() {
            return Cow::Borrowed(&self.value[self.host_index - 1..]);
        }

        if self.path_index == 0 && self.query_index == 0 {
            return Cow::Borrowed("/");
        }

        if self.path_index > 0 {
            return Cow::Borrowed(&self.value[self.path_index..]);
        }

        // Query but no path: the request target must be origin-form, so the returned
        // value must start with '/'. query_index points AT the '?', so prepend one.
        Cow::Owned(format!("/{}", &self.value[self.query_index..]))
    }
    pub fn host_is_ip(&self) -> bool {
        let host = self.get_host();
        // Accept a bracketed IPv6 literal ([::1]) as well as bare IPv4/IPv6.
        let host = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host);
        host.parse::<std::net::IpAddr>().is_ok()
    }

    pub fn get_path(&self) -> &str {
        if self.path_index == 0 {
            return "/";
        }
        if self.query_index == 0 {
            return &self.value[self.path_index..];
        }

        &self.value[self.path_index..self.query_index]
    }

    pub fn get_query(&self) -> Option<&str> {
        if self.query_index == 0 {
            return None;
        }

        let result = &self.value[self.query_index + 1..];

        Some(result)
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

/// Where the host starts when `src` starts with a scheme: right after its `://`.
///
/// A scheme is what RFC 3986 says it is — a letter, then letters, digits, '+', '-'
/// and '.' — and it is only looked for at the very start. A `://` further on belongs
/// to a path or a query (`host/path?next=http://other`), not to a scheme.
fn host_index_after_scheme(src: &str) -> Option<usize> {
    if !src.as_bytes().first()?.is_ascii_alphabetic() {
        return None;
    }

    let scheme_len = src
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')))?;

    let after_scheme = &src[scheme_len..];

    if after_scheme.starts_with("://") {
        return Some(scheme_len + 3);
    }

    // `http+unix:/~/path` — a socket in the home directory. UrlBuilder::new never
    // sends a unix scheme here; this is for a caller that builds this type itself.
    if after_scheme.starts_with(":/~") {
        let is_unix_socket = Scheme::try_parse(&src[..scheme_len])
            .map(|scheme| scheme.is_unix_socket())
            .unwrap_or(false);

        if is_unix_socket {
            return Some(scheme_len + 3);
        }
    }

    None
}

impl std::fmt::Display for UrlBuilderInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.value)
    }
}

#[cfg(test)]
mod tests {

    use crate::UrlBuilderInner;

    #[test]
    pub fn test_with_default_scheme() {
        let uri_builder = UrlBuilderInner::new("google.com".into());

        assert_eq!(uri_builder.host_index, 7);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 0);

        assert_eq!("http://google.com", uri_builder.as_str());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!("google.com", uri_builder.get_host());

        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());

        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme() {
        let uri_builder = UrlBuilderInner::new("http://google.com".into());

        assert_eq!(uri_builder.host_index, 7);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 0);

        assert_eq!("http://google.com", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme_and_last_slash() {
        let uri_builder = UrlBuilderInner::new("http://google.com/".into());

        assert_eq!(uri_builder.host_index, 7);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 17);
        assert_eq!(uri_builder.query_index, 0);

        assert_eq!("http://google.com/", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_https_scheme() {
        let uri_builder = UrlBuilderInner::new("https://google.com".into());

        assert_eq!(uri_builder.host_index, 8);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 0);

        assert_eq!("https://google.com", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segments() {
        let mut uri_builder = UrlBuilderInner::new("https://google.com".into());
        assert_eq!(uri_builder.host_index, 8);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 0);

        uri_builder.append_path_segment("first");
        assert_eq!(uri_builder.path_index, 18);
        uri_builder.append_path_segment("second");

        assert_eq!("https://google.com/first/second", uri_builder.as_str());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!("/first/second", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segments_with_slug_at_the_end() {
        let mut uri_builder = UrlBuilderInner::new("https://google.com/".into());
        assert_eq!(uri_builder.host_index, 8);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 18);
        assert_eq!(uri_builder.query_index, 0);
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        assert_eq!("https://google.com/first/second", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!("/first/second", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_query_with_no_path() {
        let mut uri_builder = UrlBuilderInner::new("https://google.com".into());
        uri_builder.append_query_param("first", Some("first_value"));
        uri_builder.append_query_param("second", Some("second_value"));

        assert_eq!(uri_builder.host_index, 8);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 18);

        assert_eq!(
            "https://google.com?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!(uri_builder.get_path(), "/",);
        assert_eq!(
            "/?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }

    #[test]
    pub fn test_get_domain_different_cases() {
        let uri_builder = UrlBuilderInner::new("https://my-domain:5123".into());

        assert_eq!("my-domain:5123", uri_builder.get_host_port());
        assert_eq!("my-domain", uri_builder.get_host());

        let uri_builder = UrlBuilderInner::new("https://my-domain:5123/my-path".into());

        assert_eq!("my-domain:5123", uri_builder.get_host_port());
        assert_eq!("my-domain", uri_builder.get_host());

        let uri_builder = UrlBuilderInner::new("https://my-domain/my-path".into());

        assert_eq!("my-domain", uri_builder.get_host_port());
        assert_eq!("my-domain", uri_builder.get_host());
    }

    #[test]
    pub fn test_path_and_query() {
        let mut uri_builder = UrlBuilderInner::new("https://google.com".into());
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        uri_builder.append_query_param("first", Some("first_value"));
        uri_builder.append_query_param("second", Some("second_value"));

        assert_eq!(
            "https://google.com/first/second?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!(
            "/first/second?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }
}
