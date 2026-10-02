use rust_extensions::remote_endpoint::{RemoteEndpoint, Scheme};

pub struct UrlBuilderUnixSocket {
    // The scheme and the slashes after it, as written — everything before the socket
    // path. Empty for a bare path.
    scheme: String,
    host: String,
    path: String,
    query: String,
}

impl UrlBuilderUnixSocket {
    /// `true` when `src` starts with a unix socket scheme — any spelling
    /// `Scheme::try_parse` takes for one, in any case — followed by a '/'. Without the
    /// slash it is a host that happens to be named like the scheme: `unix:8080`.
    pub(super) fn has_scheme(src: &str) -> bool {
        socket_path_index(src).is_some()
    }

    pub fn new(host_port: &str) -> Self {
        let (scheme, host_port) = match socket_path_index(host_port) {
            Some(index) => host_port.split_at(index),
            None => ("", host_port),
        };

        let index = host_port.find(':');

        let Some(index) = index else {
            return Self {
                scheme: scheme.to_string(),
                host: host_port.to_string(),
                path: Default::default(),
                query: Default::default(),
            };
        };

        let host = host_port[..index].to_string();

        let path_and_query = host_port[index + 1..].to_string();

        let (path, query) = match path_and_query.find('?') {
            Some(index) => {
                let path = path_and_query[..index].to_string();
                let query = path_and_query[index..].to_string();
                (path, query)
            }
            None => (path_and_query.to_string(), String::new()),
        };

        Self {
            scheme: scheme.to_string(),
            host,
            path,
            query,
        }
    }

    pub fn get_remote_endpoint<'s>(&'s self) -> RemoteEndpoint<'s> {
        RemoteEndpoint::try_parse(&self.host).unwrap()
    }

    pub fn append_path_segment(&mut self, path_segment: &str) {
        // Strip a leading '/' so we don't emit a double slash (parity with the TCP builder).
        let segment = path_segment.strip_prefix('/').unwrap_or(path_segment);
        self.path.push('/');
        // Path-segment percent-encoding (parity with the TCP builder).
        crate::url_encoder::encode_path_segment_and_copy(&mut self.path, segment);
    }

    pub fn append_query_param(&mut self, name: &str, value: Option<&str>) {
        if self.query.is_empty() {
            self.query.push('?');
        } else {
            self.query.push('&');
        }

        crate::encode_to_url_string_and_copy(&mut self.query, name);

        if let Some(value) = value {
            self.query.push('=');
            crate::encode_to_url_string_and_copy(&mut self.query, value);
        }
    }

    pub fn get_path_and_query(&self) -> String {
        let path = self.get_path();
        let mut result = String::with_capacity(path.len() + self.query.len());
        result.push_str(path);

        if !self.query.is_empty() {
            result.push_str(&self.query);
        }
        result
    }

    pub fn get_path(&self) -> &str {
        // Parity with the TCP builder: an empty path is the root "/".
        if self.path.is_empty() {
            "/"
        } else {
            &self.path
        }
    }

    pub fn get_scheme_and_host(&self) -> &str {
        &self.host
    }

    pub fn get_host(&self) -> &str {
        self.host.as_str()
    }

    pub fn append_raw_ending(&mut self, raw_ending: &str) {
        // Split off the query part (parity with the TCP builder), otherwise get_query
        // returns None and the query ends up buried inside the path.
        match raw_ending.find('?') {
            Some(index) => {
                self.path.push_str(&raw_ending[..index]);
                if self.query.is_empty() {
                    self.query.push_str(&raw_ending[index..]);
                } else {
                    // Already have a query; merge with '&' instead of a second '?'.
                    self.query.push('&');
                    self.query.push_str(&raw_ending[index + 1..]);
                }
            }
            None => {
                self.path.push_str(raw_ending);
            }
        }
    }

    pub fn get_query(&self) -> Option<&str> {
        if self.query.is_empty() {
            None
        } else {
            Some(&self.query[1..])
        }
    }

}

/// Where the socket path starts in an address with a unix socket scheme, `None` when
/// `src` has no such scheme.
///
/// Any number of slashes may follow the scheme, and the path keeps exactly one of
/// them: `unix:/run/x.sock`, `unix://run/x.sock` and `unix:///run/x.sock` all name
/// `/run/x.sock`, the same as rust-extensions reads them. A path in the home
/// directory keeps none: `http+unix:/~/x.sock` names `~/x.sock`.
fn socket_path_index(src: &str) -> Option<usize> {
    let colon = src.find(':')?;

    if !Scheme::try_parse(&src[..colon])?.is_unix_socket() {
        return None;
    }

    let slashes = src[colon + 1..].bytes().take_while(|b| *b == b'/').count();

    if slashes == 0 {
        return None;
    }

    let after_slashes = colon + 1 + slashes;

    if src[after_slashes..].starts_with('~') {
        Some(after_slashes)
    } else {
        Some(after_slashes - 1)
    }
}

impl std::fmt::Display for UrlBuilderUnixSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.scheme)?;
        f.write_str(&self.host)?;

        // The ':' is the host/path separator; omit it for a host-only URL so it
        // round-trips textually.
        if !self.path.is_empty() || !self.query.is_empty() {
            f.write_str(":")?;
        }

        if !self.path.is_empty() {
            f.write_str(&self.path)?;
        }

        if !self.query.is_empty() {
            f.write_str(&self.query)?;
        }

        Ok(())
    }
}
