//! Server-side `parse` tests. The derive-generated `parse` is exercised through an in-memory
//! `THttpRequest`, exactly as `my-http-server` will drive it — but with no hyper/tokio in sight.

use my_http_utils::http_input::{HttpParseError, THttpRequest};
use my_http_utils::macros::*;

// ---- in-memory THttpRequest -------------------------------------------------

#[derive(Default)]
struct FakeRequest {
    query: String,
    headers: Vec<(String, String)>,
    path: Vec<(String, String)>,
    body: Vec<u8>,
    content_type: Option<String>,
}

impl FakeRequest {
    fn query(mut self, q: &str) -> Self {
        self.query = q.to_string();
        self
    }
    fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }
    fn path(mut self, k: &str, v: &str) -> Self {
        self.path.push((k.to_string(), v.to_string()));
        self
    }
    fn body(mut self, content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        self.content_type = Some(content_type.to_string());
        self.body = body.into();
        self
    }
}

impl THttpRequest for FakeRequest {
    fn get_query_string(&self) -> &str {
        &self.query
    }
    fn get_header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn get_path_value(&self, name: &str) -> Option<&str> {
        self.path
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    fn get_body(&self) -> &[u8] {
        &self.body
    }
    fn get_content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }
}

fn only_digits(v: &str) -> Result<(), String> {
    if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        Err(format!("'{}' is not digits", v))
    }
}

// ---- the everything-model (path/query/header/body + Option/default/Vec/trim/case/validator) --

#[derive(Debug, Clone, Copy, MyHttpStringEnum)]
enum Color {
    #[http_enum_case(id = "0", value = "red", description = "Red", default)]
    Red,
    #[http_enum_case(id = "1", value = "green", description = "Green")]
    Green,
}

#[derive(Debug, MyHttpInput)]
struct AllSources {
    #[http_path(name = "id", description = "")]
    id: String,
    #[http_query(name = "n", description = "")]
    n: i32,
    #[http_query(name = "opt", description = "")]
    opt: Option<i64>,
    #[http_query(name = "limit", description = "", default = 10)]
    limit: i32,
    #[http_query(name = "tags", description = "")]
    tags: Vec<String>,
    #[http_query(name = "code", description = "", validator = "only_digits")]
    code: String,
    #[http_query(name = "s", description = "", trim, to_lowercase)]
    s: String,
    #[http_query(name = "color", description = "")]
    color: Color,
    #[http_header(name = "X-Api-Key", description = "")]
    api_key: String,
    #[http_header(name = "X-Trace", description = "")]
    trace: Option<String>,
    #[http_body(name = "amount", description = "")]
    amount: f64,
    #[http_body(name = "note", description = "")]
    note: Option<String>,
}

#[test]
fn all_sources_parse_ok() {
    let request = FakeRequest::default()
        .path("id", "u1")
        .query("n=5&opt=9&tags=a&tags=b&code=123&s=+HeLLo+&color=green")
        .header("X-Api-Key", "KEY")
        .body("application/json", r#"{"amount":12.5,"note":"hi"}"#);

    let model = AllSources::parse(&request).unwrap();

    assert_eq!(model.id, "u1");
    assert_eq!(model.n, 5);
    assert_eq!(model.opt, Some(9));
    assert_eq!(model.limit, 10, "absent -> default");
    assert_eq!(model.tags, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(model.code, "123");
    assert_eq!(model.s, "hello", "trim then lowercase");
    assert!(matches!(model.color, Color::Green));
    assert_eq!(model.api_key, "KEY");
    assert_eq!(model.trace, None, "absent optional header");
    assert_eq!(model.amount, 12.5);
    assert_eq!(model.note, Some("hi".to_string()));

    // READS_BODY reflects the http_body field.
    assert!(AllSources::READS_BODY);
}

#[test]
fn all_sources_default_is_overridden_when_present() {
    let request = FakeRequest::default()
        .path("id", "u1")
        .query("n=1&tags=x&code=9&s=x&color=red&limit=99")
        .header("X-Api-Key", "K")
        .body("application/json", r#"{"amount":1.0}"#);

    let model = AllSources::parse(&request).unwrap();
    assert_eq!(model.limit, 99);
    assert!(matches!(model.color, Color::Red));
    assert_eq!(model.note, None, "absent optional body field");
}

// ---- body kinds: json / url-encoded / form-data -----------------------------

#[derive(MyHttpInput)]
struct BodyModel {
    #[http_body(name = "name", description = "")]
    name: String,
    #[http_body(name = "age", description = "")]
    age: i32,
}

#[test]
fn json_body() {
    let request = FakeRequest::default().body("application/json", r#"{"name":"John","age":30}"#);
    let model = BodyModel::parse(&request).unwrap();
    assert_eq!(model.name, "John");
    assert_eq!(model.age, 30);
}

#[test]
fn url_encoded_body() {
    let request = FakeRequest::default().body(
        "application/x-www-form-urlencoded",
        "name=John+Doe&age=42",
    );
    let model = BodyModel::parse(&request).unwrap();
    assert_eq!(model.name, "John Doe");
    assert_eq!(model.age, 42);
}

#[derive(MyHttpInput)]
struct FormModel {
    #[http_form_data(name = "title", description = "")]
    title: String,
    #[http_form_data(name = "count", description = "")]
    count: i32,
}

#[test]
fn form_data_body() {
    let boundary = "TESTBOUNDARY";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nMyTitle\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"count\"\r\n\r\n5\r\n\
         --{b}--\r\n",
        b = boundary
    );
    let request = FakeRequest::default().body(
        &format!("multipart/form-data; boundary={}", boundary),
        body.into_bytes(),
    );
    let model = FormModel::parse(&request).unwrap();
    assert_eq!(model.title, "MyTitle");
    assert_eq!(model.count, 5);
}

// ---- raw body ---------------------------------------------------------------

#[derive(MyHttpInput)]
struct RawStringModel {
    #[http_body_raw(description = "")]
    body: String,
}

#[test]
fn raw_body_as_string() {
    let request = FakeRequest::default().body("text/plain", "hello raw".as_bytes().to_vec());
    let model = RawStringModel::parse(&request).unwrap();
    assert_eq!(model.body, "hello raw");
    assert!(RawStringModel::READS_BODY);
}

// ---- error cases ------------------------------------------------------------

#[derive(Debug, MyHttpInput)]
struct RequiredQuery {
    #[http_query(name = "n", description = "")]
    n: i32,
}

#[test]
fn missing_required_reports_name_and_src() {
    let request = FakeRequest::default().query("other=1");
    let err = RequiredQuery::parse(&request).unwrap_err();
    match err {
        HttpParseError::RequiredParameterIsMissing { name, src } => {
            assert_eq!(name, "n");
            assert_eq!(src, "QueryString");
        }
        other => panic!("expected RequiredParameterIsMissing, got {:?}", other),
    }
}

#[test]
fn cannot_parse_reports_name_src_value() {
    let request = FakeRequest::default().query("n=abc");
    let err = RequiredQuery::parse(&request).unwrap_err();
    match err {
        HttpParseError::CanNotParseValue { name, src, value } => {
            assert_eq!(name, "n");
            assert_eq!(src, "QueryString");
            assert_eq!(value, "abc");
        }
        other => panic!("expected CanNotParseValue, got {:?}", other),
    }
}

#[test]
fn validator_failure_is_a_validation_error() {
    let request = FakeRequest::default()
        .path("id", "u1")
        .query("n=1&tags=x&code=12a&s=x&color=red")
        .header("X-Api-Key", "K")
        .body("application/json", r#"{"amount":1.0}"#);
    let err = AllSources::parse(&request).unwrap_err();
    match err {
        HttpParseError::Validation(msg) => assert!(msg.contains("not digits"), "got {}", msg),
        other => panic!("expected Validation, got {:?}", other),
    }
}

#[test]
fn missing_required_header() {
    #[derive(Debug, MyHttpInput)]
    struct H {
        #[http_header(name = "X-Key", description = "")]
        key: String,
    }
    let err = H::parse(&FakeRequest::default()).unwrap_err();
    match err {
        HttpParseError::RequiredParameterIsMissing { name, src } => {
            assert_eq!(name, "X-Key");
            assert_eq!(src, "Header");
        }
        other => panic!("expected missing header, got {:?}", other),
    }
}

#[test]
fn missing_required_path() {
    #[derive(Debug, MyHttpInput)]
    struct P {
        #[http_path(name = "id", description = "")]
        id: String,
    }
    let err = P::parse(&FakeRequest::default()).unwrap_err();
    match err {
        HttpParseError::RequiredParameterIsMissing { name, src } => {
            assert_eq!(name, "id");
            assert_eq!(src, "Path");
        }
        other => panic!("expected missing path, got {:?}", other),
    }
}

// ---- READS_BODY on a body-less model ---------------------------------------

#[test]
fn reads_body_false_for_query_only_model() {
    assert!(!RequiredQuery::READS_BODY);
}

// ---- regressions from the adversarial review -------------------------------

use my_http_utils::http_input::{BodyReader, FileContent, QueryStringReader, RawData};

// Non-Option #[http_body_raw] must take the whole body verbatim, NOT route through the
// content-type-parsing BodyReader — which would reject a non-object JSON body even though the
// raw field's own conversion handles it fine. (`String` is used so the same model still builds
// as a client request.)
#[derive(MyHttpInput)]
struct RawStringBody {
    #[http_body_raw(description = "")]
    body: String,
}

#[test]
fn raw_body_accepts_non_object_json_under_json_content_type() {
    // A JSON array with Content-Type: application/json — the old code sniffed "json", tried to
    // parse it as a body object, and failed. Now the raw body is taken verbatim.
    let request = FakeRequest::default().body("application/json", b"[1,2,3]".to_vec());
    let model = RawStringBody::parse(&request).unwrap();
    assert_eq!(model.body, "[1,2,3]");
}

// JSON number members keep their exact source text (no f64 rounding), and a whole-value
// deserialize (Vec<u128>) survives values outside i64/u64 range. Exercised at the reader level,
// since RawData / u128 aren't client-buildable field types.
#[test]
fn json_number_precision_is_preserved() {
    let reader = BodyReader::from_parts(
        br#"{"amount":100.00,"ids":[123456789012345678901234567890]}"#,
        Some("application/json"),
    )
    .unwrap();

    let amount: String = reader.get_required("amount").unwrap().try_into().unwrap();
    assert_eq!(amount, "100.00", "exact scale, not f64-rounded 100.0");

    let ids: Vec<u128> = reader.get_required("ids").unwrap().try_into().unwrap();
    assert_eq!(ids, vec![123456789012345678901234567890u128]);
}

// RawData over a JSON member returns the verbatim source bytes (no key reordering / re-escaping).
#[test]
fn json_member_rawdata_is_verbatim() {
    let reader =
        BodyReader::from_parts(br#"{"cfg":{"b":1,"a":2}}"#, Some("application/json")).unwrap();
    let cfg: RawData = reader.get_required("cfg").unwrap().try_into().unwrap();
    assert_eq!(cfg.as_slice(), br#"{"b":1,"a":2}"#, "verbatim, not re-serialized");
}

// Reading a file out of a non-form-data value is Forbidden (403), not NotSupportedContentType.
#[test]
fn file_from_query_value_is_forbidden() {
    let query = QueryStringReader::new("f=x").unwrap();
    let value = query.get_required("f").unwrap();
    let result: Result<FileContent, _> = value.try_into();
    assert!(matches!(result, Err(HttpParseError::Forbidden(_))));
}
