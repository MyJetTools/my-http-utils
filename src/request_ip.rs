pub enum RequestIp<'s> {
    Result(String),
    Forwarded(Vec<&'s str>),
}

impl<'s> RequestIp<'s> {
    pub fn get_real_ip(&'s self) -> &'s str {
        match self {
            RequestIp::Result(ip) => ip.as_str(),
            RequestIp::Forwarded(forwarded_ips) => forwarded_ips[0],
        }
    }
}
