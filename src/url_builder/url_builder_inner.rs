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

        let mut domain_index = host_port.find("://");

        if domain_index.is_none() {
            domain_index = host_port.find(":/~");
        }

        let host_index = if let Some(domain_index) = domain_index {
            domain_index + 3
        } else {
            value.push_str("http://");
            7
        };
        value.push_str(host_port);

        let mut port_index = 0;
        let mut path_index = 0;
        let mut query_index = 0;

        // Scan by byte index. The delimiters ':' '/' '?' are ASCII, and no ASCII
        // byte ever appears inside a multi-byte UTF-8 sequence, so byte offsets are
        // safe to feed back into slicing (unlike the previous char-count positions).
        for (pos, b) in value.bytes().enumerate() {
            if pos <= host_index {
                continue;
            }

            match b {
                b':' => {
                    if path_index == 0 {
                        port_index = pos;
                    }
                }
                b'/' => {
                    if path_index == 0 {
                        path_index = pos;
                    }
                }
                b'?' => {
                    if path_index == 0 {
                        path_index = pos;
                    }
                    if query_index == 0 {
                        query_index = pos;
                        break;
                    }
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
        let mut result = RemoteEndpoint::try_parse(self.get_scheme_and_host()).unwrap();

        if let Some(default_port) = default_port {
            result.set_default_port(default_port);
        }

        result
    }

    pub fn append_path_segment(&mut self, path: &str) {
        let segment = path.strip_prefix('/').unwrap_or(path);

        // If a query has already been appended, splice the segment in before '?'
        // instead of after it (which would make path_index > query_index and panic
        // on the backwards slice in get_path).
        if self.query_index != 0 {
            let insert_at = self.query_index;
            let mut spliced =
                String::with_capacity(self.value.len() + segment.len() + 1);
            spliced.push_str(&self.value[..insert_at]);
            if !spliced.ends_with('/') {
                spliced.push('/');
            }
            if self.path_index == 0 {
                self.path_index = spliced.len() - 1;
            }
            crate::url_encoder::encode_path_segment_and_copy(&mut spliced, segment);
            self.query_index = spliced.len();
            spliced.push_str(&self.value[insert_at..]);
            self.value = spliced;
            return;
        }

        if !self.value.ends_with('/') {
            self.value.push('/');
        }
        if self.path_index == 0 {
            self.path_index = self.value.len() - 1;
        }

        crate::url_encoder::encode_path_segment_and_copy(&mut self.value, segment);
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
        if !self.value.ends_with('/') {
            self.value.push('/');
        }

        // Only record the path start if there is no path yet; overwriting it would
        // make get_path/get_path_and_query drop a pre-existing base path while
        // to_string() still keeps it.
        if self.path_index == 0 {
            self.path_index = self.value.len() - 1;
        }

        if let Some(rest) = raw_ending.strip_prefix('/') {
            self.value.push_str(rest);
        } else {
            self.value.push_str(raw_ending);
        }

        if self.query_index == 0 {
            if let Some(index) = self.value.find('?') {
                self.query_index = index;
            }
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
