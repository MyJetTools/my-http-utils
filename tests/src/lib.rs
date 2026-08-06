//! Test crate for `my-http-utils`. All macros are used via the public `my_http_utils::macros`
//! re-export (never the proc-macro crate directly), exactly as external consumers would.

#[cfg(test)]
mod body_stream_tests;
#[cfg(test)]
mod parse_tests;

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use my_http_utils::body::HttpRequestBody;
    use my_http_utils::macros::*;
    use my_http_utils::schema::client::{
        HeaderBuilder, HttpRequestBuildError, RandomStringGenerator, THttpRequestBuilder,
    };
    use my_http_utils::schema::data_types::DataTypeProvider;
    use my_http_utils::UrlBuilder;

    // ---- shared helpers ----

    #[derive(Default)]
    struct Collector(Vec<(String, String)>);

    impl HeaderBuilder for Collector {
        fn add_header(&mut self, name: &str, value: &str) {
            self.0.push((name.to_string(), value.to_string()));
        }
    }

    // Deterministic RNG for tests — a boundary only has to be absent from the body.
    struct FixedRnd;
    impl RandomStringGenerator for FixedRnd {
        fn generate_random_string(_len: usize) -> String {
            "TESTBOUNDARY0001".to_string()
        }
    }

    fn try_build_url(
        host: &str,
        m: &impl THttpRequestBuilder,
    ) -> Result<String, HttpRequestBuildError> {
        let mut u = UrlBuilder::new(host);
        m.fill_url(&mut u)?;
        Ok(u.to_string())
    }

    fn build_url(host: &str, m: &impl THttpRequestBuilder) -> String {
        try_build_url(host, m).unwrap()
    }

    fn headers_of(m: &impl THttpRequestBuilder) -> Vec<(String, String)> {
        let mut c = Collector::default();
        m.fill_headers(&mut c).unwrap();
        c.0
    }

    // ---- decorated model zoo ----

    #[derive(Clone, Copy, MyHttpStringEnum)]
    enum Color {
        #[http_enum_case(id = "0", value = "red", description = "Red", default)]
        Red,
        #[http_enum_case(id = "1", value = "green", description = "Green")]
        Green,
    }

    #[derive(Clone, Copy, MyHttpIntegerEnum)]
    enum Priority {
        #[http_enum_case(id = "1", description = "low")]
        Low,
        #[http_enum_case(id = "10", description = "high", default)]
        High,
    }

    #[http_input_field(tp: "String")]
    struct Password(String);

    #[derive(MyHttpInput)]
    struct QueryModel {
        #[http_query(name = "s", description = "")]
        s: String,
        #[http_query(name = "n", description = "")]
        n: i32,
        #[http_query(name = "b", description = "")]
        b: bool,
        #[http_query(name = "opt", description = "")]
        opt: Option<i64>,
        #[http_query(name = "list", description = "")]
        list: Vec<String>,
        #[http_query(name = "color", description = "")]
        color: Color,
        #[http_query(name = "prio", description = "")]
        prio: Priority,
    }

    #[derive(MyHttpInput)]
    struct PathModel {
        #[http_path(name = "id", description = "")]
        id: String,
        #[http_path(name = "ver", description = "")]
        ver: u32,
    }

    #[derive(MyHttpInput)]
    struct HeaderModel {
        #[http_header(name = "X-Api-Key", description = "")]
        api_key: String,
        #[http_header(name = "X-Trace", description = "")]
        trace: Option<String>,
    }

    #[derive(Serialize, MyHttpInput)]
    struct JsonBodyModel {
        #[http_body(name = "name", description = "")]
        name: String,
        #[http_body(name = "age", description = "")]
        age: Option<i32>,
    }

    // An object structure gets a my-json `JsonValueWriter` (no serde) so it can be serialised
    // as the whole body or nested inside another object. `#[serde(rename)]` still drives the key
    // (the Serialize derive is only present to register that attribute — it is not used to build).
    #[derive(Serialize, MyHttpObjectStructure)]
    struct NestedCfg {
        host: String,
        port: u16,
        #[serde(rename = "tagName")]
        tag: Option<String>,
    }

    #[derive(MyHttpInput)]
    struct FormDataModel {
        #[http_form_data(name = "title", description = "")]
        title: String,
        #[http_form_data(name = "count", description = "")]
        count: i32,
    }

    #[derive(MyHttpInput)]
    struct RawBodyModel {
        #[http_body_raw(description = "")]
        data: Vec<u8>,
    }

    // A typed raw body. `Deserialize` + `MyHttpObjectStructure` cover the server + schema; `Serialize`
    // enables the client `payload.into()` build path (serialise a value straight into the body).
    #[derive(serde::Serialize, serde::Deserialize, MyHttpObjectStructure)]
    struct RawTypedPayload {
        a: i32,
        b: String,
    }

    #[derive(MyHttpInput)]
    struct RawTypedBodyModel {
        #[http_body_raw(description = "")]
        body: my_http_utils::http_input::RawDataTyped<RawTypedPayload>,
    }

    #[derive(MyHttpInput)]
    struct EmptyBodyModel {
        #[http_query(name = "q", description = "")]
        q: String,
    }

    #[derive(Serialize, MyHttpInput)]
    struct MixedModel {
        #[http_path(name = "userId", description = "")]
        user_id: String,
        #[http_query(name = "dryRun", description = "")]
        dry_run: bool,
        #[http_header(name = "X-Token", description = "")]
        token: String,
        #[http_body(name = "amount", description = "")]
        amount: f64,
        #[http_body(name = "note", description = "")]
        note: Option<String>,
    }

    #[derive(MyHttpInput)]
    struct PasswordQuery {
        #[http_query(name = "pwd", description = "")]
        pwd: Password,
    }

    // ---- enums / custom field: shared as_str + schema ----

    #[test]
    fn string_enum_as_str_and_schema() {
        assert_eq!(Color::Red.as_str(), "red");
        assert_eq!(Color::Green.as_str(), "green");
        // DataTypeProvider is emitted (schema side), client build.
        let _ = Color::get_data_type();
    }

    #[test]
    fn integer_enum_as_str_uses_case_name_when_no_value() {
        assert_eq!(Priority::Low.as_str(), "Low");
        assert_eq!(Priority::High.as_str(), "High");
        let _ = Priority::get_data_type();
    }

    #[test]
    fn custom_field_as_str() {
        let p = Password("secret".to_string());
        assert_eq!(p.as_str(), "secret");
        let _ = Password::get_data_type();
    }

    // ---- fill_url ----

    #[test]
    fn query_model_fills_url() {
        let m = QueryModel {
            s: "a b".to_string(),
            n: -5,
            b: true,
            opt: Some(9),
            list: vec!["x".to_string(), "y".to_string()],
            color: Color::Green,
            prio: Priority::Low,
        };
        assert_eq!(
            build_url("http://h", &m),
            "http://h?s=a+b&n=-5&b=true&opt=9&list=x&list=y&color=green&prio=Low"
        );
    }

    #[test]
    fn query_option_none_is_skipped_vec_empty_is_skipped() {
        let m = QueryModel {
            s: "z".to_string(),
            n: 0,
            b: false,
            opt: None,
            list: vec![],
            color: Color::Red,
            prio: Priority::High,
        };
        assert_eq!(
            build_url("http://h", &m),
            "http://h?s=z&n=0&b=false&color=red&prio=High"
        );
    }

    #[test]
    fn path_model_appends_segments_in_order() {
        let m = PathModel {
            id: "u1".to_string(),
            ver: 3,
        };
        assert_eq!(build_url("https://api.example.com", &m), "https://api.example.com/u1/3");
    }

    #[test]
    fn custom_field_query() {
        let m = PasswordQuery {
            pwd: Password("p@ss".to_string()),
        };
        assert_eq!(build_url("http://h", &m), "http://h?pwd=p%40ss");
    }

    // ---- fill_headers ----

    #[test]
    fn header_model_fills_headers() {
        let m = HeaderModel {
            api_key: "KEY".to_string(),
            trace: Some("t-1".to_string()),
        };
        assert_eq!(
            headers_of(&m),
            vec![
                ("X-Api-Key".to_string(), "KEY".to_string()),
                ("X-Trace".to_string(), "t-1".to_string()),
            ]
        );
    }

    #[test]
    fn header_option_none_is_skipped() {
        let m = HeaderModel {
            api_key: "KEY".to_string(),
            trace: None,
        };
        assert_eq!(headers_of(&m), vec![("X-Api-Key".to_string(), "KEY".to_string())]);
    }

    // ---- get_body ----

    #[test]
    fn json_body_omits_none_option() {
        let full = JsonBodyModel {
            name: "John".to_string(),
            age: Some(30),
        }
        .get_body::<FixedRnd>().unwrap();
        assert_eq!(full.get_content_type().unwrap().as_str(), "application/json");
        let v: serde_json::Value = serde_json::from_slice(&full.into_vec()).unwrap();
        assert_eq!(v["name"], "John");
        assert_eq!(v["age"], 30);

        let partial = JsonBodyModel {
            name: "x".to_string(),
            age: None,
        }
        .get_body::<FixedRnd>().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&partial.into_vec()).unwrap();
        assert_eq!(v["name"], "x");
        assert!(v.get("age").is_none(), "None option must be omitted");
    }

    #[test]
    fn object_structure_json_value_writer() {
        use my_http_utils::my_json::json_writer::JsonValueWriter;

        // Fields render in declaration order; a `None` Option is omitted.
        let mut s = String::new();
        NestedCfg {
            host: "h".to_string(),
            port: 8080,
            tag: None,
        }
        .write(&mut s);
        assert_eq!(s, r#"{"host":"h","port":8080}"#);

        // A `Some` renames via `#[serde(rename)]` → "tagName".
        let mut s2 = String::new();
        NestedCfg {
            host: "h".to_string(),
            port: 1,
            tag: Some("blue".to_string()),
        }
        .write(&mut s2);
        assert_eq!(s2, r#"{"host":"h","port":1,"tagName":"blue"}"#);
    }

    #[test]
    fn form_data_body() {
        let body = FormDataModel {
            title: "MyTitle".to_string(),
            count: 5,
        }
        .get_body::<FixedRnd>().unwrap();
        assert!(matches!(body, HttpRequestBody::FormData(_)));
        assert!(body
            .get_content_type()
            .unwrap()
            .as_str()
            .starts_with("multipart/form-data"));
        let text = String::from_utf8(body.into_vec()).unwrap();
        assert!(text.contains("name=\"title\""));
        assert!(text.contains("MyTitle"));
        assert!(text.contains("name=\"count\""));
        assert!(text.contains("5"));
    }

    #[test]
    fn raw_body_moves_bytes_through() {
        let body = RawBodyModel {
            data: vec![1, 2, 3, 4],
        }
        .get_body::<FixedRnd>().unwrap();
        match body {
            HttpRequestBody::Raw { data, content_type } => {
                assert_eq!(data, vec![1, 2, 3, 4]);
                assert!(content_type.is_none());
            }
            _ => panic!("expected Raw"),
        }
    }

    #[test]
    fn raw_data_typed_body_bytes_are_sent_verbatim() {
        // The client raw-body branch moves the stored bytes straight out via `RawDataTyped ->
        // Into<Vec<u8>>` (no serde, no re-encoding), so a body built from raw bytes is sent
        // byte-for-byte — key order preserved (`b` precedes `a` below and stays that way).
        let stored = br#"{"b":"x","a":7}"#.to_vec();
        let body = RawTypedBodyModel {
            body: my_http_utils::http_input::RawDataTyped::from_slice(&stored, "Body"),
        }
        .get_body::<FixedRnd>()
        .unwrap();
        match body {
            HttpRequestBody::Raw { data, content_type } => {
                assert_eq!(data, stored, "verbatim JSON, not re-serialized/reordered");
                assert_eq!(content_type, Some("application/json"));
            }
            _ => panic!("expected Raw"),
        }
    }

    #[test]
    fn raw_data_typed_client_from_value_serializes_into_body() {
        // The client builds RawDataTyped<T> straight from a typed value via `into()` (which
        // serialises the value inside the `From<T>`); get_body then sends those JSON bytes as the
        // body, tagged application/json.
        let payload = RawTypedPayload {
            a: 7,
            b: "x".to_string(),
        };
        let body = RawTypedBodyModel { body: payload.into() }
            .get_body::<FixedRnd>()
            .unwrap();
        match body {
            HttpRequestBody::Raw { data, content_type } => {
                let v: serde_json::Value = serde_json::from_slice(&data).unwrap();
                assert_eq!(v["a"], 7);
                assert_eq!(v["b"], "x");
                assert_eq!(content_type, Some("application/json"));
            }
            _ => panic!("expected Raw"),
        }
    }

    #[test]
    fn no_body_is_empty() {
        let body = EmptyBodyModel { q: "x".to_string() }.get_body::<FixedRnd>().unwrap();
        assert!(matches!(body, HttpRequestBody::Empty));
        assert!(body.get_content_type().is_none());
    }

    // ---- mixed end-to-end + schema surface ----

    #[test]
    fn mixed_model_end_to_end() {
        let m = MixedModel {
            user_id: "u1".to_string(),
            dry_run: false,
            token: "tok".to_string(),
            amount: 12.5,
            note: Some("hi".to_string()),
        };
        assert_eq!(build_url("https://api", &m), "https://api/u1?dryRun=false");
        assert_eq!(headers_of(&m), vec![("X-Token".to_string(), "tok".to_string())]);

        let v: serde_json::Value = serde_json::from_slice(&m.get_body::<FixedRnd>().unwrap().into_vec()).unwrap();
        assert_eq!(v["amount"], 12.5);
        assert_eq!(v["note"], "hi");
    }

    #[test]
    fn schema_surface_is_present() {
        // shared schema side of the same derive
        assert_eq!(QueryModel::get_input_params().len(), 7);
        assert_eq!(MixedModel::get_model_routes(), Some(vec!["userId"]));
        assert_eq!(PathModel::get_model_routes(), Some(vec!["id", "ver"]));
        assert_eq!(QueryModel::get_model_routes(), None);
    }

    // ---- restored model directives: trim / case / validator / default / print ----

    #[derive(MyHttpInput)]
    struct TransformModel {
        #[http_query(name = "s", description = "", trim, to_lowercase)]
        s: String,
        #[http_query(name = "u", description = "", to_uppercase)]
        u: String,
        #[http_header(name = "X-H", description = "", trim)]
        h: String,
    }

    #[test]
    fn directives_trim_and_case_are_applied() {
        let m = TransformModel {
            s: "  HeLLo  ".to_string(),
            u: "abc".to_string(),
            h: "  v  ".to_string(),
        };
        // s: trim -> "HeLLo", then lowercase -> "hello"; u: uppercase -> "ABC"
        assert_eq!(build_url("http://h", &m), "http://h?s=hello&u=ABC");
        // header h: trim -> "v"
        assert_eq!(headers_of(&m), vec![("X-H".to_string(), "v".to_string())]);
    }

    fn only_digits(v: &str) -> Result<(), String> {
        if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
            Ok(())
        } else {
            Err(format!("'{}' is not digits", v))
        }
    }

    #[derive(MyHttpInput)]
    struct ValidatedModel {
        #[http_query(name = "code", description = "", validator = "only_digits")]
        code: String,
    }

    #[test]
    fn validator_passes_for_valid_value() {
        let m = ValidatedModel {
            code: "123".to_string(),
        };
        assert_eq!(build_url("http://h", &m), "http://h?code=123");
    }

    #[test]
    fn validator_returns_err_for_invalid_value() {
        let m = ValidatedModel {
            code: "12a".to_string(),
        };
        let err = try_build_url("http://h", &m).unwrap_err();
        assert_eq!(err.field, "code");
        assert!(err.reason.contains("not digits"), "reason was {}", err.reason);
    }

    #[derive(MyHttpInput)]
    struct DefaultModel {
        #[http_query(name = "limit", description = "", default = 10)]
        limit: i32,
    }

    #[test]
    fn default_parses_and_client_sends_actual_value() {
        // `default` is a schema/server concern; the client sends the field's actual value.
        let m = DefaultModel { limit: 5 };
        assert_eq!(build_url("http://h", &m), "http://h?limit=5");
        assert_eq!(DefaultModel::get_input_params().len(), 1);
    }

    #[derive(MyHttpInput)]
    struct DebugModel {
        #[http_query(name = "q", description = "", print_request_to_console)]
        q: String,
    }

    #[test]
    fn print_directive_still_builds_correct_url() {
        let m = DebugModel {
            q: "x".to_string(),
        };
        assert_eq!(build_url("http://h", &m), "http://h?q=x");
    }
}
