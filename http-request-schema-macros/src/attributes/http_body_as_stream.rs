use types_reader::macros::*;

/// `#[http_body_as_stream]` — the whole body is read as a stream of chunks into this one field
/// (typed `my_http_utils::http_input::HttpBodyAsStream`).
///
/// Only `name` / `description`: the value never exists as a string, so the outgoing-value
/// directives the other field attributes carry (`trim`, `to_lowercase`, `to_uppercase`,
/// `validator`, `default`, `print_request_to_console`) have nothing to act on here.
#[attribute_name("http_body_as_stream")]
#[derive(MacrosParameters, Clone)]
pub struct HttpBodyAsStreamAttribute<'s> {
    pub name: Option<&'s str>,
    pub description: &'s str,
}
