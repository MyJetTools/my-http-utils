use std::net::SocketAddr;

use crate::{
    http_path::{GetPathValueResult, PathSegments},
    HttpFailResult, QueryString, RequestIp, WebContentType,
};
use hyper::{Body, Method, Request};

pub struct HttpContext {
    req: Request<Body>,
    path_lower_case: String,
    addr: SocketAddr,
    pub route: Option<PathSegments>,
}

impl HttpContext {
    pub fn new(req: Request<Body>, addr: SocketAddr) -> Self {
        let path_lower_case = req.uri().path().to_lowercase();
        Self {
            req,
            path_lower_case,
            addr,
            route: None,
        }
    }

    pub fn get_value_from_path(&self, key: &str) -> Result<&str, HttpFailResult> {
        let path = self.get_path();

        if self.route.is_none() {
            return Err(HttpFailResult::as_forbidden(Some(format!(
                "Path [{}] does not has keys in it",
                path
            ))));
        }

        let route = self.route.as_ref().unwrap();

        match route.get_value(path, key) {
            GetPathValueResult::Value(value) => Ok(value),
            GetPathValueResult::NoKeyInTheRoute => Err(HttpFailResult::as_forbidden(Some(
                format!("Route [{}] does not have key[{}]", route.path, key),
            ))),
            GetPathValueResult::NoValue => Err(HttpFailResult::as_forbidden(Some(format!(
                "Route [{}] does not have value for the path [{}] with the key [{}]",
                route.path,
                self.get_path(),
                key
            )))),
        }
    }

    pub fn get_value_from_path_optional(&self, key: &str) -> Result<Option<&str>, HttpFailResult> {
        let path = self.get_path();

        if self.route.is_none() {
            return Err(HttpFailResult::as_forbidden(Some(format!(
                "Path [{}] does not has keys in it",
                path
            ))));
        }

        let route = self.route.as_ref().unwrap();

        match route.get_value(path, key) {
            GetPathValueResult::Value(value) => Ok(Some(value)),
            GetPathValueResult::NoValue => Ok(None),
            GetPathValueResult::NoKeyInTheRoute => Err(HttpFailResult::as_forbidden(Some(
                format!("Route [{}] does not have key[{}]", route.path, key),
            ))),
        }
    }

    pub fn get_path(&self) -> &str {
        self.req.uri().path()
    }

    pub fn get_path_lower_case(&self) -> &str {
        &self.path_lower_case
    }

    pub fn get_ip(&self) -> RequestIp {
        let headers = self.req.headers();
        let ip_header = headers.get("X-Forwarded-For");

        if let Some(ip_value) = ip_header {
            let forwared_ip = std::str::from_utf8(ip_value.as_bytes()).unwrap();

            let result: Vec<&str> = forwared_ip.split(",").map(|itm| itm.trim()).collect();

            return RequestIp::Forwarded(result);
        }

        return RequestIp::Result(self.addr.to_string());
    }

    pub fn get_required_header(&self, header_name: &str) -> Result<&str, HttpFailResult> {
        for (name, value) in self.req.headers() {
            if name.as_str() == header_name {
                return Ok(value.to_str().unwrap());
            }
        }

        return Err(HttpFailResult::as_header_parameter_required(
            format!("Header: {}", header_name).as_str(),
        ));
    }

    pub async fn get_form_data(self) -> Result<QueryString, HttpFailResult> {
        let body = self.req.into_body();
        let full_body = hyper::body::to_bytes(body).await.unwrap();

        let result = full_body.iter().cloned().collect::<Vec<u8>>();
        let a = String::from_utf8(result).unwrap();

        match QueryString::new(a.as_str()) {
            Ok(result) => return Ok(result),
            Err(err) => {
                let result = HttpFailResult {
                    metric_it: true,
                    content: format!("Can not parse Form Data. {:?}", err).into_bytes(),
                    content_type: WebContentType::Text,
                    status_code: 412,
                };

                return Err(result);
            }
        }
    }

    pub fn get_method(&self) -> &Method {
        self.req.method()
    }

    pub fn get_query_string(&self) -> Result<QueryString, HttpFailResult> {
        let query = self.req.uri().query();

        match query {
            Some(query) => Ok(QueryString::new(query)?),
            None => Err(HttpFailResult::as_forbidden(Some(
                "No query string found".to_string(),
            ))),
        }
    }

    pub fn get_host(&self) -> &str {
        std::str::from_utf8(&self.req.headers().get("host").unwrap().as_bytes()).unwrap()
    }

    pub fn get_scheme(&self) -> String {
        let headers = self.req.headers();
        let proto_header = headers.get("X-Forwarded-Proto");

        if let Some(scheme) = proto_header {
            let bytes = scheme.as_bytes();
            return String::from_utf8(bytes.to_vec()).unwrap();
        }

        let scheme = self.req.uri().scheme();

        match scheme {
            Some(scheme) => {
                return scheme.to_string();
            }
            None => "http".to_string(),
        }
    }
}
