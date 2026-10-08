//! `parse_with_body_stream` — the derive-generated parse that takes the body as a
//! `rust_extensions::AsyncBytesStream` instead of `THttpRequest::get_body`. This is what
//! my-http-server hands an incoming body to; here a stream of chunks cut wherever a test likes
//! stands in for it.
//!
//! The yardstick is `parse` over the same body, whole: a body that comes as a stream must read
//! into the same model, however it is cut.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use my_http_utils::http_input::{HttpBodyAsStream, HttpParseError, RawData};
use my_http_utils::macros::*;
use my_http_utils::rust_extensions::AsyncBytesStream;

use crate::parse_tests::FakeRequest;

// ---- a body as a stream -----------------------------------------------------

/// A body handed over chunk by chunk, the way a transport reads it off a connection. It counts
/// how many times it is asked, so a test can tell how much of the body a parse read.
struct Chunks<TError> {
    chunks: Mutex<VecDeque<Result<Vec<u8>, TError>>>,
    size: Option<usize>,
    requests: Arc<AtomicUsize>,
}

impl Chunks<HttpParseError> {
    /// The body cut into chunks of `chunk_size` bytes.
    fn split(body: &[u8], chunk_size: usize) -> Self {
        Self::new(
            body.chunks(chunk_size)
                .map(|chunk| Ok(chunk.to_vec()))
                .collect(),
        )
    }
}

impl<TError> Chunks<TError> {
    fn new(chunks: Vec<Result<Vec<u8>, TError>>) -> Self {
        Self {
            chunks: Mutex::new(chunks.into()),
            size: None,
            requests: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn with_size(mut self, size: usize) -> Self {
        self.size = Some(size);
        self
    }

    /// Stays with the test once the stream is moved into a parse.
    fn requests(&self) -> Arc<AtomicUsize> {
        self.requests.clone()
    }
}

#[async_trait::async_trait]
impl<TError: Send + 'static> AsyncBytesStream<TError> for Chunks<TError> {
    type Chunk = Vec<u8>;

    async fn get_next(&self) -> Result<Option<Vec<u8>>, TError> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.chunks.lock().unwrap().pop_front().transpose()
    }

    fn get_size(&self) -> Option<usize> {
        self.size
    }
}

/// A transport's own error: the stream fails with it, and `Into` makes it a parse error.
#[derive(Debug)]
struct ConnectionLost;

impl From<ConnectionLost> for HttpParseError {
    fn from(_: ConnectionLost) -> Self {
        HttpParseError::BodyStream("connection lost".to_string())
    }
}

// ---- models -----------------------------------------------------------------

#[derive(Debug, MyHttpInputObjectStructure)]
struct Client {
    name: String,
    age: i32,
}

#[derive(Debug, MyHttpInput)]
struct OrderInput {
    #[http_path(name = "id", description = "")]
    id: String,
    #[http_query(name = "dry_run", description = "", default = false)]
    dry_run: bool,
    #[http_header(name = "X-Api-Key", description = "")]
    api_key: String,
    #[http_body(name = "amount", description = "")]
    amount: f64,
    #[http_body(name = "note", description = "", trim)]
    note: Option<String>,
    #[http_body(name = "tags", description = "")]
    tags: Vec<String>,
    #[http_body(name = "client", description = "")]
    client: Client,
}

const ORDER: &str =
    r#"{"amount":12.50,"note":"  for later ","tags":["a","b"],"client":{"name":"John","age":42}}"#;

/// Everything but the body. Its own body is something else entirely, so a test fails if the
/// streamed parse ever reads it instead of the stream.
fn order_request() -> FakeRequest {
    FakeRequest::default()
        .path("id", "o1")
        .query("dry_run=true")
        .header("X-Api-Key", "KEY")
        .body("application/json", "not the body")
}

fn order_request_with_body(body: &str) -> FakeRequest {
    FakeRequest::default()
        .path("id", "o1")
        .query("dry_run=true")
        .header("X-Api-Key", "KEY")
        .body("application/json", body)
}

/// What `parse` makes of the whole body, and what `parse_with_body_stream` makes of it cut into
/// chunks of every size — they must agree.
async fn assert_order_reads_like_the_whole_body(body: &str) -> OrderInput {
    let expected = format!(
        "{:?}",
        OrderInput::parse(&order_request_with_body(body)).unwrap()
    );

    for chunk_size in [1, 2, 3, 7, 16, body.len()] {
        let model = OrderInput::parse_with_body_stream(
            &order_request(),
            Chunks::split(body.as_bytes(), chunk_size),
        )
        .await
        .unwrap_or_else(|err| panic!("chunks of {} bytes: {:?}", chunk_size, err));

        assert_eq!(
            format!("{:?}", model),
            expected,
            "chunks of {} bytes",
            chunk_size
        );
    }

    OrderInput::parse_with_body_stream(&order_request(), Chunks::split(body.as_bytes(), 5))
        .await
        .unwrap()
}

// ---- a JSON body ------------------------------------------------------------

#[tokio::test]
async fn a_json_body_cut_anywhere_reads_like_the_whole_body() {
    let model = assert_order_reads_like_the_whole_body(ORDER).await;

    assert_eq!(model.id, "o1");
    assert!(model.dry_run);
    assert_eq!(model.api_key, "KEY");
    assert_eq!(model.amount, 12.5);
    assert_eq!(model.note.as_deref(), Some("for later"));
    assert_eq!(model.tags, ["a", "b"]);
    assert_eq!(model.client.name, "John");
    assert_eq!(model.client.age, 42);
}

#[tokio::test]
async fn members_the_model_does_not_name_are_passed_by() {
    let model = assert_order_reads_like_the_whole_body(
        r#"{ "skip": {"deep": [1, 2, {"x": "}"}]}, "amount": 1, "other": "x\"y}",
             "tags": [], "client": {"name": "A", "age": 1}, "note": null }"#,
    )
    .await;

    assert_eq!(model.amount, 1.0);
    // `null` is absent, exactly as in a whole body
    assert_eq!(model.note, None);
}

#[tokio::test]
async fn the_first_of_two_members_with_one_key_is_read() {
    let model = assert_order_reads_like_the_whole_body(
        r#"{"amount":1,"amount":2,"tags":[],"client":{"name":"A","age":1}}"#,
    )
    .await;

    assert_eq!(model.amount, 1.0);
}

#[tokio::test]
async fn a_key_is_matched_with_its_escapes_resolved() {
    let model = assert_order_reads_like_the_whole_body(
        r#"{"amount":5,"tags":[],"client":{"name":"A","age":1}}"#,
    )
    .await;

    assert_eq!(model.amount, 5.0);
}

#[tokio::test]
async fn the_body_is_not_read_past_the_last_member_the_model_names() {
    // Everything the model names is in the first chunk. The second one would fail the parse —
    // it must never be asked for.
    let stream = Chunks::new(vec![
        Ok(br#"{"amount":1,"tags":[],"client":{"name":"A","age":1},"note":"n","#.to_vec()),
        Err(HttpParseError::BodyStream("must not be read".to_string())),
    ]);
    let requests = stream.requests();

    let model = OrderInput::parse_with_body_stream(&order_request(), stream)
        .await
        .unwrap();

    assert_eq!(model.note.as_deref(), Some("n"));
    assert_eq!(requests.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn a_missing_member_is_reported_as_the_whole_body_reports_it() {
    let body = r#"{"tags":[],"client":{"name":"A","age":1}}"#;

    let expected = OrderInput::parse(&order_request_with_body(body)).unwrap_err();
    let err = OrderInput::parse_with_body_stream(&order_request(), Chunks::split(body.as_bytes(), 4))
        .await
        .unwrap_err();

    assert_eq!(err, expected);
    assert_eq!(err, HttpParseError::required("amount", "Body"));
}

#[tokio::test]
async fn a_body_which_is_not_an_object_fails_as_the_whole_body_fails() {
    let body = "[1, 2]";

    let expected = OrderInput::parse(&order_request_with_body(body)).unwrap_err();
    let err = OrderInput::parse_with_body_stream(&order_request(), Chunks::split(body.as_bytes(), 2))
        .await
        .unwrap_err();

    assert_eq!(err, expected);
}

#[tokio::test]
async fn an_empty_json_body_is_an_invalid_body() {
    let err = OrderInput::parse_with_body_stream(&order_request(), Chunks::split(b"", 1))
        .await
        .unwrap_err();

    assert!(
        matches!(err, HttpParseError::InvalidBodyFormat(_)),
        "{:?}",
        err
    );
}

#[tokio::test]
async fn a_malformed_json_body_fails_as_soon_as_the_broken_part_arrives() {
    let body = r#"{"skip": tru, "amount": 1, "tags": [], "client": {"name": "A", "age": 1}}"#;

    let err = OrderInput::parse_with_body_stream(&order_request(), Chunks::split(body.as_bytes(), 3))
        .await
        .unwrap_err();

    assert!(
        matches!(err, HttpParseError::InvalidBodyFormat(_)),
        "{:?}",
        err
    );
}

#[tokio::test]
async fn an_error_of_the_stream_is_handed_over_through_into() {
    let stream = Chunks::new(vec![Ok(br#"{"amount":1"#.to_vec()), Err(ConnectionLost)]);

    let err = OrderInput::parse_with_body_stream(&order_request(), stream)
        .await
        .unwrap_err();

    assert_eq!(err, HttpParseError::BodyStream("connection lost".to_string()));
}

// ---- the sources before the body ---------------------------------------------

#[tokio::test]
async fn a_request_that_fails_on_its_headers_does_not_wait_for_its_body() {
    let request = FakeRequest::default()
        .path("id", "o1")
        .body("application/json", "");
    let stream = Chunks::split(ORDER.as_bytes(), 4);
    let requests = stream.requests();

    let err = OrderInput::parse_with_body_stream(&request, stream)
        .await
        .unwrap_err();

    assert_eq!(err, HttpParseError::required("X-Api-Key", "Header"));
    assert_eq!(requests.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn the_parse_can_run_on_any_thread() {
    // `tokio::spawn` takes only a `Send` future: the parse holds the stream and the my-json
    // reader across its awaits, and they must not make it `!Send`.
    let amount = tokio::spawn(async {
        let model =
            OrderInput::parse_with_body_stream(&order_request(), Chunks::split(ORDER.as_bytes(), 5))
                .await?;
        Ok::<_, HttpParseError>(model.amount)
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(amount, 12.5);
}

// ---- bodies that are read whole -----------------------------------------------

#[derive(Debug, MyHttpInput)]
struct NameAgeInput {
    #[http_body(name = "name", description = "")]
    name: String,
    #[http_body(name = "age", description = "")]
    age: i32,
}

async fn assert_name_age_reads_like_the_whole_body(content_type: Option<&str>, body: &[u8]) {
    let request = match content_type {
        Some(content_type) => FakeRequest::default().body(content_type, body.to_vec()),
        None => FakeRequest::default().untyped_body(body.to_vec()),
    };

    let expected = NameAgeInput::parse(&request).unwrap();

    for chunk_size in [1, 3, body.len()] {
        let model = NameAgeInput::parse_with_body_stream(&request, Chunks::split(body, chunk_size))
            .await
            .unwrap();

        assert_eq!(format!("{:?}", model), format!("{:?}", expected));
    }
}

#[tokio::test]
async fn a_url_encoded_body_reads_like_the_whole_body() {
    assert_name_age_reads_like_the_whole_body(
        Some("application/x-www-form-urlencoded"),
        b"name=John+Doe&age=42",
    )
    .await;
}

#[tokio::test]
async fn a_body_with_no_content_type_is_told_apart_as_the_whole_body_is() {
    assert_name_age_reads_like_the_whole_body(None, br#"{"name":"John","age":42}"#).await;
    assert_name_age_reads_like_the_whole_body(None, b"name=John&age=42").await;
}

#[derive(Debug, MyHttpInput)]
struct FormInput {
    #[http_form_data(name = "title", description = "")]
    title: String,
    #[http_form_data(name = "count", description = "")]
    count: i32,
}

#[tokio::test]
async fn a_multipart_body_reads_like_the_whole_body() {
    let boundary = "TESTBOUNDARY";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nMyTitle\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"count\"\r\n\r\n5\r\n\
         --{b}--\r\n",
        b = boundary
    );
    let request = FakeRequest::default().body(
        &format!("multipart/form-data; boundary={}", boundary),
        "not the body",
    );

    let model = FormInput::parse_with_body_stream(&request, Chunks::split(body.as_bytes(), 7))
        .await
        .unwrap();

    assert_eq!(model.title, "MyTitle");
    assert_eq!(model.count, 5);
}

#[derive(Debug, MyHttpInput)]
struct RawBodyInput {
    #[http_body_raw(description = "")]
    body: Vec<u8>,
}

#[tokio::test]
async fn a_raw_body_is_the_whole_stream_as_it_is() {
    // Not JSON, not text — a raw body is never parsed
    let body: Vec<u8> = (0..=255).collect();
    let request = FakeRequest::default().body("application/octet-stream", "not the body");

    let model = RawBodyInput::parse_with_body_stream(&request, Chunks::split(&body, 10))
        .await
        .unwrap();

    assert_eq!(model.body, body);
}

#[derive(Debug, MyHttpInput)]
struct OptionalRawMemberInput {
    #[http_body_raw(description = "")]
    cfg: Option<String>,
}

#[tokio::test]
async fn an_optional_raw_member_is_kept_verbatim() {
    let body = r#"{"x":1,"cfg":{"b":1, "a":2}}"#;
    let request = FakeRequest::default().body("application/json", "not the body");

    let model =
        OptionalRawMemberInput::parse_with_body_stream(&request, Chunks::split(body.as_bytes(), 3))
            .await
            .unwrap();

    // Byte for byte — no key reordering, no whitespace dropped
    assert_eq!(model.cfg.as_deref(), Some(r#"{"b":1, "a":2}"#));

    // A raw body all the same, though it is read by name
    const { assert!(OptionalRawMemberInput::READS_BODY_RAW) };
}

#[tokio::test]
async fn a_raw_member_reads_the_bytes_a_whole_body_gives() {
    use my_http_utils::http_input::core::BodyFromStream;

    let body = r#"{"x":1,"cfg":{"b":1, "a":2}}"#;
    let received = BodyFromStream::read(
        Chunks::split(body.as_bytes(), 2),
        Some("application/json"),
        &["cfg"],
    )
    .await
    .unwrap();

    let reader = received.get_body_reader().unwrap();
    let cfg: RawData = reader.get_required("cfg").unwrap().try_into().unwrap();
    assert_eq!(cfg.as_slice(), br#"{"b":1, "a":2}"#);
}

// ---- #[http_body_as_stream]: the stream goes into the field --------------------

#[derive(MyHttpInput)]
struct UploadInput {
    #[http_header(name = "X-File-Name", description = "")]
    file_name: String,
    #[http_body_as_stream(description = "")]
    body: HttpBodyAsStream,
}

#[tokio::test]
async fn a_streamed_body_field_gets_the_stream_unread() {
    let request = FakeRequest::default().header("X-File-Name", "report.bin");
    let stream = Chunks::split(b"0123456789", 4).with_size(10);
    let requests = stream.requests();

    let model = UploadInput::parse_with_body_stream(&request, stream)
        .await
        .unwrap();

    assert_eq!(model.file_name, "report.bin");
    assert_eq!(model.body.get_content_length(), Some(10));
    // `parse` read nothing out of it — the handler reads the body
    assert_eq!(requests.load(Ordering::Relaxed), 0);

    let reader = model.body.get_body_reader().unwrap();
    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"0123".to_vec()));
    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"4567".to_vec()));
    assert_eq!(reader.read_to_end(None).await.unwrap(), b"89".to_vec());
    assert_eq!(reader.get_next_chunk().await.unwrap(), None);
}

#[tokio::test]
async fn a_streamed_body_is_read_on_as_a_stream_of_bytes() {
    use my_http_utils::my_json::json_reader::JsonArrayIteratorAsync;

    let body = br#"[{"id":1},{"id":2},{"id":3}]"#;
    let request = FakeRequest::default().header("X-File-Name", "items.json");

    let model = UploadInput::parse_with_body_stream(&request, Chunks::split(body, 4))
        .await
        .unwrap();

    // The reader is an `AsyncBytesStream` itself, so my-json takes the array apart as it arrives
    let mut items = JsonArrayIteratorAsync::new(model.body.get_body_reader().unwrap());
    let mut ids = Vec::new();

    while let Some(item) = items.get_next().await {
        let item = item.unwrap();
        let id = my_http_utils::my_json::j_path::get_value(item.as_slice(), "id")
            .unwrap()
            .unwrap();
        ids.push(id.unwrap_as_number().unwrap().unwrap());
    }

    assert_eq!(ids, [1, 2, 3]);
}

// ---- no body ------------------------------------------------------------------

#[derive(Debug, MyHttpInput)]
struct NoBodyInput {
    #[http_query(name = "id", description = "")]
    id: String,
}

#[tokio::test]
async fn a_model_with_no_body_fields_leaves_the_stream_unread() {
    let request = FakeRequest::default().query("id=7");
    let stream = Chunks::split(ORDER.as_bytes(), 4);
    let requests = stream.requests();

    let model = NoBodyInput::parse_with_body_stream(&request, stream)
        .await
        .unwrap();

    assert_eq!(model.id, "7");
    assert_eq!(requests.load(Ordering::Relaxed), 0);
}
