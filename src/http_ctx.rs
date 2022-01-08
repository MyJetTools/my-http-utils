use std::net::SocketAddr;

use crate::{HttpFailResult, QueryString, RequestIp, WebContentType};
use hyper::{Body, Method, Request};

pub struct HttpContext {
    req: Request<Body>,
    path: String,
    addr: SocketAddr,
}

impl HttpContext {
    pub fn new(req: Request<Body>, addr: SocketAddr) -> Self {
        let path = req.uri().path().to_lowercase();
        Self { req, path, addr }
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

    pub fn get_path(&self) -> &str {
        self.path.as_str()
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
