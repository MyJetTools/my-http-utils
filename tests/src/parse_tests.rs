//! Server-side `parse` tests. The derive-generated `parse` is exercised through an in-memory
//! `THttpRequest`, exactly as `my-http-server` will drive it — but with no hyper/tokio in sight.

use my_http_utils::http_input::core::THttpRequest;
use my_http_utils::http_input::HttpParseError;
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

use my_http_utils::http_input::core::{BodyReader, QueryStringReader};
use my_http_utils::http_input::{FileContent, RawData, RawDataTyped};

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

// ---- PasswordHttpInputField: the utils-owned custom field --------------------

use my_http_utils::http_input::PasswordHttpInputField;
use my_http_utils::schema::data_types::{DataTypeProvider, HttpDataType, HttpSimpleType};

// The full public surface the previous server-side field exposed.
#[test]
fn password_field_public_api() {
    // new(String) + as_str()
    let p = PasswordHttpInputField::new("secret".to_string());
    assert_eq!(p.as_str(), "secret");

    // Deref<Target = str> — str methods work directly.
    assert_eq!(p.len(), 6);
    assert!(p.starts_with("sec"));

    // Into<String>
    let s: String = p.into();
    assert_eq!(s, "secret");

    // From<String>
    let p: PasswordHttpInputField = "pw".to_string().into();
    // into_string()
    assert_eq!(p.into_string(), "pw");

    // DataTypeProvider → Password (Swagger renders it masked).
    assert!(matches!(
        PasswordHttpInputField::get_data_type(),
        HttpDataType::SimpleType(HttpSimpleType::Password)
    ));
}

// Server parse: a model whose field is the utils-owned PasswordHttpInputField, driven exactly
// like my-http-server will drive it. Proves the derive-emitted `TryFrom<HttpInputValue>` wires up.
#[derive(Debug, MyHttpInput)]
struct LoginModel {
    #[http_query(name = "password", description = "")]
    password: PasswordHttpInputField,
}

#[test]
fn password_field_parses_from_request() {
    let request = FakeRequest::default().query("password=hunter2");
    let model = LoginModel::parse(&request).unwrap();
    assert_eq!(model.password.as_str(), "hunter2");
}

// ---- #[http_body_raw]: the raw path now moves the verbatim body Vec<u8> ------

// `Vec<u8>` raw body = the bytes as-is (no JSON decode), symmetric with the client builder.
#[derive(MyHttpInput)]
struct RawBytesModel {
    #[http_body_raw(description = "")]
    data: Vec<u8>,
}

#[test]
fn raw_body_vec_u8_is_verbatim_bytes() {
    // Arbitrary binary that is NOT valid JSON (the old path tried to JSON-decode this and failed).
    let raw = vec![0u8, 1, 2, 255, 254, 3, b'{'];
    let request = FakeRequest::default().body("application/octet-stream", raw.clone());
    let model = RawBytesModel::parse(&request).unwrap();
    assert_eq!(model.data, raw, "verbatim bytes, not JSON-decoded");
}

// RawData builds from raw bytes infallibly. Direct `Vec<u8>` → `RawData` conversions still exist
// (`From<Vec<u8>>` and its blanket `TryFrom`); the server derive itself now builds the field via
// `FromRawBody` (see from_raw_body.rs), but these remain for direct use.
#[test]
fn raw_data_from_vec_is_infallible_and_verbatim() {
    let raw = vec![9u8, 8, 7, 0, 200];
    let rd: RawData = raw.clone().into();
    assert_eq!(rd.as_slice(), raw.as_slice());
    // the blanket `TryFrom<Vec<u8>>` (Error = Infallible) that comes from that `From` also holds:
    let rd2: RawData = raw.clone().try_into().unwrap();
    assert_eq!(rd2.as_slice(), raw.as_slice());
}

// RawDataTyped is built infallibly from the raw bytes; the JSON error surfaces only from
// deserialize_json(), never at construction.
#[test]
fn raw_data_typed_defers_json_error_to_deserialize() {
    // Built from raw body bytes (the server direction) — infallible, no JSON parse yet.
    let good: RawDataTyped<Vec<i32>> = RawDataTyped::from_slice(b"[1,2,3]", "Body");
    assert_eq!(good.deserialize_json().unwrap(), vec![1, 2, 3]);

    // Construction of an ill-formed payload still succeeds (infallible)...
    let bad: RawDataTyped<Vec<i32>> = RawDataTyped::from_slice(b"not json at all", "Body");
    // ...and only deserialize_json() reports the JSON error.
    match bad.deserialize_json() {
        Err(HttpParseError::CanNotParseValue { .. }) => {}
        other => panic!("expected a JSON parse error from deserialize_json, got {:?}", other),
    }
}

// ---- #[http_body_raw] RawDataTyped<Model>: a typed raw body -----------------

// A model that is `Deserialize + MyHttpObjectStructure`. Its structure feeds the OpenAPI schema of
// the typed raw body, and `RawDataTyped<T>` deserializes the whole body into it on demand. Note it
// derives no `Serialize` — the whole point is that a typed raw body needs only `Deserialize`.
#[derive(serde::Deserialize, MyHttpObjectStructure)]
struct AuditQueryParams {
    account_id: String,
    limit: i32,
}

// `#[http_body_raw] body: RawDataTyped<AuditQueryParams>` must compile (schema + client + server),
// read the whole body verbatim into `RawDataTyped`, and hand OpenAPI the inner model's structure.
#[derive(MyHttpInput)]
struct QueryAuditInput {
    #[http_body_raw(description = "Audit query filter")]
    body: RawDataTyped<AuditQueryParams>,
}

#[test]
fn raw_data_typed_body_parses_and_deserializes() {
    let request =
        FakeRequest::default().body("application/json", r#"{"account_id":"acc-1","limit":50}"#);
    let model = QueryAuditInput::parse(&request).unwrap();

    // The whole body is captured verbatim (no content-type parsing / re-encoding)...
    assert_eq!(
        model.body.as_slice(),
        br#"{"account_id":"acc-1","limit":50}"#
    );

    // ...and deserialize_json() yields the typed model, unchanged from the derive's own path.
    let params = model.body.deserialize_json().unwrap();
    assert_eq!(params.account_id, "acc-1");
    assert_eq!(params.limit, 50);

    assert!(QueryAuditInput::READS_BODY);
}

#[test]
fn raw_data_typed_body_schema_is_the_inner_models_object() {
    // The OpenAPI schema for the raw-body param is the inner model's Object structure — not bytes.
    let params = QueryAuditInput::get_input_params();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].field.name, "body");
    assert!(
        matches!(params[0].field.data_type, HttpDataType::Object(_)),
        "RawDataTyped<Model> schema must be the model's Object, got {:?}",
        params[0].field.data_type
    );
}


// ---- nested object bodies (the read half of `MyHttpInputObjectStructure`) ----------------------
//
// A `#[http_body]` field typed as a nested object is written into the request by the client's
// `JsonValueWriter` and read back out of it by serde. These tests pin BOTH halves, and — in
// `nested_object_body_client_server_round_trip` — that the two agree on the wire, which is the
// property that silently broke when `#[serde(rename_all)]` was ignored.

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
struct PciDssBankCardsModel {
    card_number: String,
    exp_month: String,
}

#[derive(MyHttpInput)]
struct PayInput {
    #[http_body(name = "challengeId", description = "Challenge id")]
    challenge_id: String,
    #[http_body(name = "pciDssBankCards", description = "Card payment instrument")]
    pci_dss_bank_cards: Option<PciDssBankCardsModel>,
}

#[test]
fn server_parses_nested_object_body() {
    let request = FakeRequest::default().body(
        "application/json",
        r#"{"challengeId":"c1","pciDssBankCards":{"card_number":"4111111111111111","exp_month":"12"}}"#,
    );

    let model = PayInput::parse(&request).unwrap();

    assert_eq!(model.challenge_id, "c1");
    assert_eq!(
        model.pci_dss_bank_cards,
        Some(PciDssBankCardsModel {
            card_number: "4111111111111111".to_string(),
            exp_month: "12".to_string(),
        })
    );
}

#[test]
fn absent_nested_object_body_is_none() {
    let request = FakeRequest::default().body("application/json", r#"{"challengeId":"c1"}"#);

    let model = PayInput::parse(&request).unwrap();

    assert_eq!(model.challenge_id, "c1");
    assert!(model.pci_dss_bank_cards.is_none());
}

#[test]
fn required_nested_object_body_parses() {
    #[derive(MyHttpInput)]
    struct RequiredNested {
        #[http_body(name = "card", description = "Card")]
        card: PciDssBankCardsModel,
    }

    let request = FakeRequest::default().body(
        "application/json",
        r#"{"card":{"card_number":"4111","exp_month":"01"}}"#,
    );

    let model = RequiredNested::parse(&request).unwrap();
    assert_eq!(model.card.card_number, "4111");
    assert_eq!(model.card.exp_month, "01");
}

#[test]
fn malformed_nested_object_body_reports_the_member_name() {
    // A nested member that is not the right shape must fail as a parse error naming the member,
    // not panic and not silently default.
    let request = FakeRequest::default().body(
        "application/json",
        r#"{"challengeId":"c1","pciDssBankCards":{"card_number":"4111"}}"#,
    );

    match PayInput::parse(&request).err() {
        Some(HttpParseError::CanNotParseValue { name, src, .. }) => {
            assert_eq!(name, "pciDssBankCards");
            assert_eq!(src, "BodyJson");
        }
        other => panic!("expected CanNotParseValue for pciDssBankCards, got {:?}", other),
    }
}

// ---- client -> server round trip ---------------------------------------------------------------

struct NoRnd;
impl my_http_utils::schema::client::RandomStringGenerator for NoRnd {
    fn generate_random_string(_len: usize) -> String {
        "TESTBOUNDARY0001".to_string()
    }
}

/// Builds `model` exactly as the client would, and hands the bytes back as an incoming request —
/// so a key the writer invents but the reader never looks for shows up as a failing test.
fn round_trip(model: impl my_http_utils::schema::client::THttpRequestBuilder) -> FakeRequest {
    match model.get_body::<NoRnd>().unwrap() {
        my_http_utils::body::HttpRequestBody::Json(data) => {
            FakeRequest::default().body("application/json", data)
        }
        _ => panic!("expected a JSON body"),
    }
}

#[test]
fn nested_object_body_client_server_round_trip() {
    let sent = PayInput {
        challenge_id: "c1".to_string(),
        pci_dss_bank_cards: Some(PciDssBankCardsModel {
            card_number: "4111111111111111".to_string(),
            exp_month: "12".to_string(),
        }),
    };

    let parsed = PayInput::parse(&round_trip(sent)).unwrap();

    assert_eq!(parsed.challenge_id, "c1");
    assert_eq!(
        parsed.pci_dss_bank_cards,
        Some(PciDssBankCardsModel {
            card_number: "4111111111111111".to_string(),
            exp_month: "12".to_string(),
        })
    );
}

#[test]
fn absent_nested_object_round_trips_as_none() {
    let sent = PayInput {
        challenge_id: "c1".to_string(),
        pci_dss_bank_cards: None,
    };

    let parsed = PayInput::parse(&round_trip(sent)).unwrap();

    assert_eq!(parsed.challenge_id, "c1");
    assert!(parsed.pci_dss_bank_cards.is_none());
}

// ---- #[serde(rename_all)] ----------------------------------------------------------------------

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
#[serde(rename_all = "camelCase")]
struct RenamedCard {
    card_number: String,
    exp_month: String,
}

#[derive(MyHttpInput)]
struct RenamedPayInput {
    #[http_body(name = "card", description = "Card")]
    card: RenamedCard,
}

#[test]
fn rename_all_round_trips_through_the_real_parse() {
    // The regression test for the wire break: with rename_all ignored the client wrote
    // {"card_number":..} while serde demanded {"cardNumber":..}, and this failed with
    // `missing field \`card_number\``.
    let sent = RenamedPayInput {
        card: RenamedCard {
            card_number: "4111".to_string(),
            exp_month: "12".to_string(),
        },
    };

    let parsed = RenamedPayInput::parse(&round_trip(sent)).unwrap();

    assert_eq!(parsed.card.card_number, "4111");
    assert_eq!(parsed.card.exp_month, "12");
}

#[test]
fn rename_all_client_body_uses_the_serde_key_names() {
    let model = RenamedPayInput {
        card: RenamedCard {
            card_number: "4111".to_string(),
            exp_month: "12".to_string(),
        },
    };

    use my_http_utils::schema::client::THttpRequestBuilder;

    let my_http_utils::body::HttpRequestBody::Json(body) = model.get_body::<NoRnd>().unwrap()
    else {
        panic!("expected a JSON body")
    };

    assert_eq!(
        String::from_utf8(body).unwrap(),
        r#"{"card":{"cardNumber":"4111","expMonth":"12"}}"#
    );
}

#[test]
fn rename_all_schema_uses_the_serde_key_names() {
    // Swagger must document the name that actually goes on the wire.
    let fields = <RenamedCard as my_http_utils::schema::data_types::DataTypeProvider>::get_data_type();
    let my_http_utils::schema::data_types::HttpDataType::Object(obj) = fields else {
        panic!("expected an object structure")
    };
    let names: Vec<&str> = obj.main.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["cardNumber", "expMonth"]);
}

/// The generated keys must equal serde's own, byte for byte, for every `rename_all` rule — this is
/// the whole client/server contract for a nested object. Serde is the reference implementation
/// here, so it is asserted against directly rather than against hand-written expectations.
///
/// Two rules are counter-intuitive and are the reason this test compares rather than assumes:
/// for *fields* (unlike enum variants) serde treats `lowercase` and `snake_case` as no-ops.
#[test]
fn rename_all_matches_serde() {
    use my_http_utils::my_json::json_writer::JsonValueWriter;

    macro_rules! assert_matches_serde {
        ($name:ident, $rule:literal, { $($field:ident),+ $(,)? }) => {{
            #[derive(serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
            #[serde(rename_all = $rule)]
            #[allow(non_snake_case)]
            struct $name { $($field: u32),+ }

            let value = $name { $($field: 1),+ };

            let mut client = String::new();
            value.write(&mut client);

            assert_eq!(
                client,
                serde_json::to_string(&value).unwrap(),
                "rename_all = {} : the client writer disagrees with serde",
                $rule,
            );
        }};
    }

    assert_matches_serde!(Lower, "lowercase", { card_number, alreadyCamel, HTTPStatus });
    assert_matches_serde!(Upper, "UPPERCASE", { card_number, exp_month });
    assert_matches_serde!(Pascal, "PascalCase", { card_number, user_id_2, field2name });
    assert_matches_serde!(Camel, "camelCase", { card_number, exp_month, a, id });
    assert_matches_serde!(Snake, "snake_case", { alreadyCamel, HTTPStatus });
    assert_matches_serde!(Screaming, "SCREAMING_SNAKE_CASE", { card_number, user_id_2 });
    assert_matches_serde!(Kebab, "kebab-case", { card_number, double__underscore });
    assert_matches_serde!(ScreamingKebab, "SCREAMING-KEBAB-CASE", { card_number, exp_month });
}

/// A field-level `#[serde(rename)]` overrides the container rule and is NOT itself re-cased, and a
/// raw identifier is keyed without its `r#` — both exactly as serde does them.
#[test]
fn field_rename_and_raw_ident_match_serde() {
    use my_http_utils::my_json::json_writer::JsonValueWriter;

    #[derive(serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
    #[serde(rename_all = "camelCase")]
    struct Precedence {
        card_number: u32,
        #[serde(rename = "EXPLICIT_wins")]
        exp_month: u32,
        security_code: u32,
    }

    let value = Precedence { card_number: 1, exp_month: 2, security_code: 3 };
    let mut client = String::new();
    value.write(&mut client);
    assert_eq!(client, serde_json::to_string(&value).unwrap());
    assert_eq!(client, r#"{"cardNumber":1,"EXPLICIT_wins":2,"securityCode":3}"#);

    #[derive(serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
    struct RawIdent {
        r#type: u32,
    }

    let value = RawIdent { r#type: 1 };
    let mut client = String::new();
    value.write(&mut client);
    assert_eq!(client, serde_json::to_string(&value).unwrap());
    assert_eq!(client, r#"{"type":1}"#);
}

/// serde params the derive does not care about must be walked over, not choke it — in particular
/// the **list form** (`bound(..)`, or a container `rename(..)` naming the struct rather than its
/// fields), which is not a `name = value` and once made the attribute parser give up with a
/// spurious `expected ','`.
#[test]
fn unrelated_serde_params_are_tolerated() {
    use my_http_utils::my_json::json_writer::JsonValueWriter;

    /// A doc comment on the container.
    #[derive(serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
    #[serde(bound(deserialize = "u32: serde::Deserialize<'de>"), rename_all = "camelCase")]
    #[serde(expecting = "an object")]
    #[serde(deny_unknown_fields)]
    struct ListForm {
        card_number: u32,
        #[serde(default, rename = "kept")]
        b: u32,
    }

    let value = ListForm { card_number: 1, b: 2 };
    let mut client = String::new();
    value.write(&mut client);

    // `bound(..)` walked over, `rename_all` still applied, field `rename` still wins.
    assert_eq!(client, serde_json::to_string(&value).unwrap());
    assert_eq!(client, r#"{"cardNumber":1,"kept":2}"#);

    // A container-level `rename(serialize = .., deserialize = ..)` renames the STRUCT, not its
    // fields, so it must be ignored here rather than rejected.
    #[derive(serde::Serialize, serde::Deserialize, MyHttpObjectStructure)]
    #[serde(rename(serialize = "Aa", deserialize = "Bb"), rename_all = "camelCase")]
    struct ContainerRename {
        card_number: u32,
    }

    let value = ContainerRename { card_number: 1 };
    let mut client = String::new();
    value.write(&mut client);
    assert_eq!(client, serde_json::to_string(&value).unwrap());
    assert_eq!(client, r#"{"cardNumber":1}"#);
}

/// A date has exactly ONE spelling on the wire, and it is the sortable one.
///
/// `DateTimeAsMicroseconds` is written into a body by `my-json`'s `JsonValueWriter` and read back
/// by serde, so the two must agree byte for byte — they did not while this crate's codegen
/// special-cased the type and called `to_rfc3339()` itself, emitting `…+00:00` against serde's
/// `…Z`. Asserting against `serde_json` rather than a literal keeps the two pinned together.
#[test]
fn date_time_wire_format_matches_serde() {
    use my_http_utils::my_json::json_writer::JsonValueWriter;
    use rust_extensions::date_time::DateTimeAsMicroseconds;

    #[derive(serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
    struct WithDate {
        dt: DateTimeAsMicroseconds,
        od: Option<DateTimeAsMicroseconds>,
        v: Vec<DateTimeAsMicroseconds>,
    }

    // Microseconds present: a floating-precision format would drop them.
    let dt = DateTimeAsMicroseconds::from_str("2024-01-02T03:04:05.123456").unwrap();
    let value = WithDate { dt, od: Some(dt), v: vec![dt] };

    let mut client = String::new();
    value.write(&mut client);

    assert_eq!(client, serde_json::to_string(&value).unwrap());
    assert!(
        client.contains("2024-01-02T03:04:05.123456Z"),
        "expected the fixed-width `Z` spelling, got {}",
        client
    );
}

/// Fixed-width microseconds + `Z` is what makes lexicographic order match chronological order —
/// the reason this spelling was chosen over `to_rfc3339()`'s floating precision.
#[test]
fn date_time_wire_format_is_sortable_as_text() {
    use rust_extensions::date_time::DateTimeAsMicroseconds;

    let mut moments: Vec<DateTimeAsMicroseconds> = [
        "2024-01-02T03:04:05.9",
        "2024-01-02T03:04:05",
        "2024-01-02T03:04:05.123456",
        "2023-12-31T23:59:59.000001",
    ]
    .iter()
    .map(|src| DateTimeAsMicroseconds::from_str(src).unwrap())
    .collect();

    moments.sort_by_key(|m| m.unix_microseconds);
    let chronological: Vec<String> = moments.iter().map(|m| m.to_rfc3339_utc()).collect();

    let mut lexicographic = chronological.clone();
    lexicographic.sort();

    assert_eq!(chronological, lexicographic);
}

/// A date in a query string / header does not go through `my-json` (those sinks take a `&str`), so
/// the spelling is named explicitly in the client writer — pin it to the same one, and pin that the
/// server reads it back exactly.
#[test]
fn date_time_in_query_round_trips() {
    use my_http_utils::schema::client::THttpRequestBuilder;
    use rust_extensions::date_time::DateTimeAsMicroseconds;

    #[derive(MyHttpInput)]
    struct QueryWithDate {
        #[http_query(name = "from", description = "From")]
        from: DateTimeAsMicroseconds,
    }

    let from = DateTimeAsMicroseconds::from_str("2024-01-02T03:04:05.123456").unwrap();

    let mut url = my_http_utils::UrlBuilder::new("https://api.example.com");
    QueryWithDate { from }.fill_url(&mut url).unwrap();
    let url = url.to_string();

    // Percent-encoded on the way out (`:` -> %3A), so assert on what the server decodes.
    let query = url.split('?').nth(1).unwrap();
    let parsed = QueryWithDate::parse(&FakeRequest::default().query(query)).unwrap();

    assert_eq!(parsed.from.unix_microseconds, from.unix_microseconds);

    // Pin the *spelling*, not just the round trip: both spellings parse back exactly, so a
    // value-only assertion here would pass against `to_rfc3339()` too and would not hold this
    // sink to the same format the body and serde use.
    assert_eq!(
        my_http_utils::url_encoded_data_reader::UrlEncodedDataReader::new(query)
            .unwrap()
            .get_required("from")
            .unwrap()
            .as_string()
            .unwrap(),
        from.to_rfc3339_utc(),
    );
}

// ---- enums nested in an object structure -------------------------------------------------------
//
// An enum nested in a `#[http_body]` object is written by `JsonValueWriter` (the `http_enum_case`
// value) but read back by serde. They only agree because the derive owns serde too — a user's
// `#[derive(Deserialize)]` would key off the Rust variant name and fail with
// `unknown variant \`bright-red\`, expected \`BrightRed\``.

// No `#[derive(Serialize, Deserialize)]` on these: the enum derive emits them.
#[derive(Debug, Clone, Copy, PartialEq, MyHttpStringEnum)]
enum NestedColor {
    #[http_enum_case(id = "0", value = "bright-red", description = "Red", default)]
    BrightRed,
    #[http_enum_case(id = "1", value = "deep-blue", description = "Blue")]
    DeepBlue,
}

#[derive(Debug, Clone, Copy, PartialEq, MyHttpIntegerEnum)]
enum NestedLevel {
    #[http_enum_case(id = "5", value = "five", description = "Five", default)]
    Five,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
struct WithEnums {
    c: NestedColor,
    l: NestedLevel,
    oc: Option<NestedColor>,
    v: Vec<NestedColor>,
}

#[derive(MyHttpInput)]
struct EnumBody {
    #[http_body(name = "n", description = "Nested")]
    n: WithEnums,
}

#[test]
fn nested_enum_wire_format_matches_serde() {
    use my_http_utils::my_json::json_writer::JsonValueWriter;

    let value = WithEnums {
        c: NestedColor::BrightRed,
        l: NestedLevel::Five,
        oc: Some(NestedColor::DeepBlue),
        v: vec![NestedColor::DeepBlue],
    };

    let mut client = String::new();
    value.write(&mut client);

    assert_eq!(client, serde_json::to_string(&value).unwrap());
    // The `http_enum_case` value, not the Rust variant name — this is what the schema documents.
    assert_eq!(
        client,
        r#"{"c":"bright-red","l":"five","oc":"deep-blue","v":["deep-blue"]}"#
    );
}

#[test]
fn nested_enum_round_trips_through_the_real_parse() {
    let sent = EnumBody {
        n: WithEnums {
            c: NestedColor::BrightRed,
            l: NestedLevel::Five,
            oc: Some(NestedColor::DeepBlue),
            v: vec![NestedColor::DeepBlue, NestedColor::BrightRed],
        },
    };

    let parsed = EnumBody::parse(&round_trip(sent)).unwrap();

    assert_eq!(parsed.n.c, NestedColor::BrightRed);
    assert_eq!(parsed.n.l, NestedLevel::Five);
    assert_eq!(parsed.n.oc, Some(NestedColor::DeepBlue));
    assert_eq!(
        parsed.n.v,
        vec![NestedColor::DeepBlue, NestedColor::BrightRed]
    );
}

#[test]
fn nested_enum_deserialize_takes_value_or_id() {
    // `TryFrom<HttpInputValue>` accepts the case value or its id; serde must not invent a second,
    // narrower behaviour. The id is accepted both as text and as a JSON number.
    for body in [
        r#"{"c":"bright-red","l":"five","oc":null,"v":[]}"#,
        r#"{"c":"0","l":"5","oc":null,"v":[]}"#,
        r#"{"c":0,"l":5,"oc":null,"v":[]}"#,
    ] {
        let parsed: WithEnums = serde_json::from_str(body).unwrap_or_else(|e| panic!("{}: {}", body, e));
        assert_eq!(parsed.c, NestedColor::BrightRed);
        assert_eq!(parsed.l, NestedLevel::Five);
    }
}

#[test]
fn nested_enum_rejects_an_unknown_case() {
    let err = serde_json::from_str::<WithEnums>(r#"{"c":"nope","l":"five","oc":null,"v":[]}"#)
        .expect_err("an unknown case must not silently become the default");

    let err = err.to_string();
    assert!(err.contains("nope") && err.contains("bright-red"), "unhelpful error: {}", err);
}

// ---- #[json_name] and the serde-free path ------------------------------------------------------

/// `#[json_name]` is this crate's own way to name a field on the wire. A `#[http_body]` object is
/// written and read entirely by the derive (my-json on both sides), so a model needs no serde at
/// all — and should not have to derive it merely to register `#[serde(rename)]`.
#[test]
fn json_name_sets_the_key_on_both_halves() {
    use my_http_utils::my_json::json_writer::JsonValueWriter;

    // Deliberately no `Serialize` / `Deserialize`: the point is that neither is needed.
    #[derive(Debug, MyHttpInputObjectStructure)]
    struct JsonNamed {
        #[json_name("cardNumber")]
        card_number: String,
        untouched: u32,
    }

    #[derive(MyHttpInput)]
    struct Body {
        #[http_body(name = "card", description = "Card")]
        card: JsonNamed,
    }

    let mut written = String::new();
    JsonNamed { card_number: "4111".to_string(), untouched: 7 }.write(&mut written);
    assert_eq!(written, r#"{"cardNumber":"4111","untouched":7}"#);

    // The reader resolves the key through the same `json_name`, so the round trip closes.
    let parsed = Body::parse(&round_trip(Body {
        card: JsonNamed { card_number: "4111".to_string(), untouched: 7 },
    }))
    .unwrap();

    assert_eq!(parsed.card.card_number, "4111");
    assert_eq!(parsed.card.untouched, 7);
}

/// `#[json_name]` wins over the container's `rename_all`, exactly as an explicit `#[serde(rename)]`
/// does — an explicit name is explicit, and is not then re-cased.
#[test]
fn json_name_overrides_rename_all() {
    use my_http_utils::my_json::json_writer::JsonValueWriter;

    #[derive(serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
    #[serde(rename_all = "camelCase")]
    struct Mixed {
        #[json_name("EXPLICIT_wins")]
        exp_month: u32,
        security_code: u32,
    }

    let mut written = String::new();
    Mixed { exp_month: 1, security_code: 2 }.write(&mut written);

    assert_eq!(written, r#"{"EXPLICIT_wins":1,"securityCode":2}"#);
}

/// A `#[http_body]` object no longer touches serde, so `Deserialize` is not required — this is the
/// property that makes `MyHttpInputObjectStructure` a drop-in for a client-only model crate.
#[test]
fn nested_object_parses_without_any_serde_derive() {
    #[derive(Debug, MyHttpInputObjectStructure)]
    struct NoSerde {
        card_number: String,
        exp_month: u32,
    }

    #[derive(MyHttpInput)]
    struct NoSerdeBody {
        #[http_body(name = "card", description = "Card")]
        card: NoSerde,
        #[http_body(name = "opt", description = "Optional")]
        opt: Option<NoSerde>,
    }

    let request = FakeRequest::default().body(
        "application/json",
        r#"{"card":{"card_number":"4111","exp_month":12}}"#,
    );

    let model = NoSerdeBody::parse(&request).unwrap();

    assert_eq!(model.card.card_number, "4111");
    assert_eq!(model.card.exp_month, 12);
    assert!(model.opt.is_none());
}

/// serde params this path cannot honour must be rejected, not ignored: a `#[http_body]` object is
/// written and read by the derive itself, so `#[serde(skip_serializing)]` would silently *send* a
/// field its author marked never-to-be-sent. These are covered by compile-fail expectations in
/// `tests/README` terms; here we pin the *positive* half — that the legal battery still compiles
/// and still round-trips, so the rejection is not over-broad.
#[test]
fn shape_neutral_serde_params_are_still_accepted() {
    use my_http_utils::my_json::json_writer::JsonValueWriter;

    #[derive(Debug, serde::Serialize, serde::Deserialize, MyHttpInputObjectStructure)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    #[serde(bound(deserialize = "u32: serde::Deserialize<'de>"))]
    struct Legal {
        card_number: u32,
        #[serde(default)]
        with_default: u32,
        #[serde(alias = "second_name")]
        last_name: String,
        #[json_name("EXPLICIT")]
        explicit: u32,
    }

    let value = Legal {
        card_number: 1,
        with_default: 2,
        last_name: "x".to_string(),
        explicit: 3,
    };

    let mut written = String::new();
    value.write(&mut written);

    // `rename_all` applied, `json_name` wins, `default`/`alias`/`bound`/`deny_unknown_fields`
    // are shape-neutral and simply pass through.
    assert_eq!(
        written,
        r#"{"cardNumber":1,"withDefault":2,"lastName":"x","EXPLICIT":3}"#
    );
}
