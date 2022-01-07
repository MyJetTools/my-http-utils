use std::collections::HashMap;

use hyper::{Body, Request};

pub struct HttpHeaders<'s> {
    data: HashMap<&'s str, &'s str>,
}

impl<'s> HttpHeaders<'s> {
    pub fn new(req: &'s Request<Body>) -> Self {
        let mut data = HashMap::new();
        for (name, value) in req.headers() {
            data.insert(name.as_str(), value.to_str().unwrap());
        }

        Self { data }
    }

    pub fn get(&self, header_name: &str) -> Option<&&str> {
        self.data.get(header_name)
    }
}
