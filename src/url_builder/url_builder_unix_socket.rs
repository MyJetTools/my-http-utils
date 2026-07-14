use rust_extensions::remote_endpoint::RemoteEndpoint;

pub struct UrlBuilderUnixSocket {
    has_scheme: bool,
    host: String,
    path: String,
    query: String,
}

impl UrlBuilderUnixSocket {
    pub fn new(host_port: &str) -> Self {
        let mut has_scheme = false;
        let host_port = if let Some(rest) = host_port.strip_prefix("http+unix:/") {
            has_scheme = true;
            rest
        } else {
            host_port
        };

        let index = host_port.find(':');

        let Some(index) = index else {
            return Self {
                has_scheme,
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
            has_scheme,
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

impl std::fmt::Display for UrlBuilderUnixSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.has_scheme {
            f.write_str("http+unix:/")?;
        }

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
