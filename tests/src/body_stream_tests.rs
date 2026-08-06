//! `#[http_body_as_stream]` — the markup, the derive-generated `parse`, and the channel behind
//! [`HttpBodyAsStream`].
//!
//! The pump here (`tokio::spawn` filling the sending half) stands in for my-http-server, which
//! does the same thing over `hyper::body::Incoming`. my-http-utils itself never spawns anything.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use my_http_utils::body::HttpRequestBody;
use my_http_utils::http_input::core::THttpRequest;
use my_http_utils::http_input::{HttpBodyAsStream, HttpBodyReader, HttpParseError};
use my_http_utils::macros::*;
use my_http_utils::schema::client::{RandomStringGenerator, THttpRequestBuilder};
use my_http_utils::UrlBuilder;

// ---- in-memory THttpRequest that CAN hand a stream over ---------------------
//
// `take_body_stream` takes `&self`, so the stream lives behind interior mutability. `Cell::take`
// is enough here — `parse` is synchronous and this request is never shared across threads.

#[derive(Default)]
struct FakeStreamRequest {
    query: String,
    headers: Vec<(String, String)>,
    stream: Cell<Option<HttpBodyAsStream>>,
}

impl FakeStreamRequest {
    fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.to_string(), v.to_string()));
        self
    }
    fn query(mut self, q: &str) -> Self {
        self.query = q.to_string();
        self
    }
    fn stream(self, stream: HttpBodyAsStream) -> Self {
        self.stream.set(Some(stream));
        self
    }
}

impl THttpRequest for FakeStreamRequest {
    fn get_query_string(&self) -> &str {
        &self.query
    }
    fn get_header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn get_path_value(&self, _name: &str) -> Option<&str> {
        None
    }
    fn get_body(&self) -> &[u8] {
        &[]
    }
    fn take_body_stream(&self) -> Option<HttpBodyAsStream> {
        self.stream.take()
    }
}

// ---- models -----------------------------------------------------------------

#[derive(MyHttpInput)]
pub struct UploadHttpInput {
    #[http_header(name = "X-File-Name", description = "File name")]
    pub file_name: String,

    #[http_query(name = "overwrite", description = "Overwrite an existing file", default = false)]
    pub overwrite: bool,

    #[http_body_as_stream(description = "File content")]
    pub body: HttpBodyAsStream,
}

#[derive(MyHttpInput)]
pub struct PlainBodyHttpInput {
    #[http_body(name = "name", description = "User name")]
    pub name: String,
}

#[derive(MyHttpInput)]
pub struct NoBodyHttpInput {
    #[http_query(name = "id", description = "Id")]
    pub id: String,
}

// ---- 1. markup and the generated consts -------------------------------------

#[test]
fn streaming_model_reports_streams_body_and_not_reads_body() {
    assert!(UploadHttpInput::STREAMS_BODY);
    assert!(!UploadHttpInput::READS_BODY);
}

#[test]
fn non_streaming_models_report_streams_body_false() {
    // STREAMS_BODY is emitted for every model, so the server codegen that reads it compiles
    // regardless of the model's shape — and READS_BODY is unchanged for the existing kinds.
    assert!(!PlainBodyHttpInput::STREAMS_BODY);
    assert!(PlainBodyHttpInput::READS_BODY);

    assert!(!NoBodyHttpInput::STREAMS_BODY);
    assert!(!NoBodyHttpInput::READS_BODY);
}

#[test]
fn streaming_model_describes_the_body_as_binary_in_the_schema() {
    let params = UploadHttpInput::get_input_params();
    let body = params
        .iter()
        .find(|p| p.field.name == "body")
        .expect("the stream field must be described");

    assert!(body.source.is_body());
    assert_eq!(body.description, "File content");
    assert!(body.field.required);
    // Same shape as `#[http_body_raw] RawData`: a streamed body carries no inner model, so
    // OpenAPI describes it as `type: string, format: binary`.
    assert!(body.field.data_type.is_binary());
}

// ---- 2. end-to-end parse ----------------------------------------------------

#[tokio::test]
async fn parse_hands_the_stream_over_and_chunks_arrive_in_order() {
    let (sender, stream) = HttpBodyAsStream::create(4, Some(9));

    let request = FakeStreamRequest::default()
        .header("X-File-Name", "report.bin")
        .query("overwrite=true")
        .stream(stream);

    let model = UploadHttpInput::parse(&request).unwrap();

    assert_eq!(model.file_name, "report.bin");
    assert!(model.overwrite);
    assert_eq!(model.body.get_content_length(), Some(9));

    // The pump — this is what my-http-server does over hyper frames.
    tokio::spawn(async move {
        for chunk in [b"aaa".to_vec(), b"bbb".to_vec(), b"ccc".to_vec()] {
            assert!(sender.send_chunk(chunk).await);
        }
        sender.finish();
    });

    let reader = model.body.get_body_reader().unwrap();
    assert_eq!(reader.get_content_length(), Some(9));

    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"aaa".to_vec()));
    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"bbb".to_vec()));
    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"ccc".to_vec()));
    assert_eq!(reader.get_next_chunk().await.unwrap(), None);
    // Repeated reads past the end stay at the clean end.
    assert_eq!(reader.get_next_chunk().await.unwrap(), None);
}

#[tokio::test]
async fn parse_fails_when_the_request_has_no_stream_to_give() {
    let request = FakeStreamRequest::default().header("X-File-Name", "report.bin");

    match UploadHttpInput::parse(&request) {
        Err(HttpParseError::BodyStream(msg)) => {
            assert_eq!(msg, "Body stream is not available");
        }
        other => panic!("expected BodyStream error, got {:?}", other.err()),
    }
}

#[tokio::test]
async fn take_body_stream_gives_the_stream_only_once() {
    let (_sender, stream) = HttpBodyAsStream::create(4, None);
    let request = FakeStreamRequest::default().stream(stream);

    assert!(request.take_body_stream().is_some());
    assert!(request.take_body_stream().is_none());
}

// ---- 3. an aborted pump must NOT look like a clean end ----------------------

#[tokio::test]
async fn a_dropped_sender_without_finish_is_an_error_not_eof() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let reader = stream.get_body_reader().unwrap();

    tokio::spawn(async move {
        assert!(sender.send_chunk(b"first".to_vec()).await);
        // Dropped here WITHOUT `finish()` — as if the pump task died mid-body.
    });

    assert_eq!(
        reader.get_next_chunk().await.unwrap(),
        Some(b"first".to_vec())
    );

    match reader.get_next_chunk().await {
        Err(HttpParseError::BodyStream(msg)) => {
            assert_eq!(msg, "Request body stream ended unexpectedly");
        }
        other => panic!("silent truncation! expected an error, got {:?}", other),
    }
}

// ---- 4. an error in the middle of the body ---------------------------------

#[tokio::test]
async fn an_error_from_the_pump_reaches_the_consumer_at_its_place() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let reader = stream.get_body_reader().unwrap();

    tokio::spawn(async move {
        assert!(sender.send_chunk(b"first".to_vec()).await);
        sender
            .send_error(HttpParseError::BodyStream("connection reset".to_string()))
            .await;
    });

    assert_eq!(
        reader.get_next_chunk().await.unwrap(),
        Some(b"first".to_vec())
    );

    match reader.get_next_chunk().await {
        Err(HttpParseError::BodyStream(msg)) => assert_eq!(msg, "connection reset"),
        other => panic!("expected the pump's error, got {:?}", other),
    }
}

// ---- 5. back pressure -------------------------------------------------------

#[tokio::test]
async fn the_pump_can_not_run_ahead_of_the_consumer() {
    const BUFFER: usize = 1;

    let (sender, stream) = HttpBodyAsStream::create(BUFFER, None);
    let sent = Arc::new(AtomicUsize::new(0));

    let sent_by_pump = sent.clone();
    tokio::spawn(async move {
        for i in 0..3u8 {
            if !sender.send_chunk(vec![i; 4]).await {
                return;
            }
            sent_by_pump.fetch_add(1, Ordering::SeqCst);
        }
        sender.finish();
    });

    let reader = stream.get_body_reader().unwrap();

    // Give the pump every chance to run away with all three chunks. It can't: the channel holds
    // `BUFFER` of them and `send().await` parks on the rest.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        sent.load(Ordering::SeqCst) <= BUFFER + 1,
        "pump ran ahead: {} chunks sent before a single one was read",
        sent.load(Ordering::SeqCst)
    );

    let mut received = 0usize;
    while let Some(chunk) = reader.get_next_chunk().await.unwrap() {
        received += 1;
        assert_eq!(chunk.len(), 4);

        tokio::time::sleep(Duration::from_millis(10)).await;

        let sent_now = sent.load(Ordering::SeqCst);
        assert!(
            sent_now <= received + BUFFER + 1,
            "pump ran {} ahead of the consumer ({} sent, {} received)",
            sent_now - received,
            sent_now,
            received
        );
    }

    assert_eq!(received, 3);
    assert_eq!(sent.load(Ordering::SeqCst), 3);
}

// ---- 6. the reader went away ------------------------------------------------

#[tokio::test]
async fn dropping_the_reader_is_signalled_both_ways() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);

    let reader = stream.get_body_reader().unwrap();
    assert!(sender.send_chunk(b"first".to_vec()).await);

    drop(reader); // the handler gave up half-way through the upload

    // (a) the next chunk can not be delivered
    assert!(
        !sender.send_chunk(b"second".to_vec()).await,
        "send_chunk must report that the reader is gone"
    );

    // (b) and a pump with nothing to send yet learns it too — this is the one that matters when
    //     the client goes quiet and there is no next chunk to try.
    tokio::time::timeout(Duration::from_secs(5), sender.closed())
        .await
        .expect("closed() must resolve once the reader is dropped");
}

#[tokio::test]
async fn closed_resolves_for_a_pump_that_never_sent_anything() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let reader = stream.get_body_reader().unwrap();

    let waiting = tokio::spawn(async move {
        sender.closed().await;
    });

    drop(reader);

    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .expect("closed() must resolve")
        .unwrap();
}

// ---- 7. read_to_end and its safety valve ------------------------------------

#[tokio::test]
async fn read_to_end_collects_the_whole_body() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let reader = stream.get_body_reader().unwrap();

    tokio::spawn(async move {
        for chunk in [b"aa".to_vec(), b"bb".to_vec(), b"cc".to_vec()] {
            assert!(sender.send_chunk(chunk).await);
        }
        sender.finish();
    });

    assert_eq!(reader.read_to_end(Some(1024)).await.unwrap(), b"aabbcc");
}

#[tokio::test]
async fn read_to_end_breaks_on_the_size_limit() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let reader = stream.get_body_reader().unwrap();

    tokio::spawn(async move {
        for _ in 0..10 {
            if !sender.send_chunk(vec![7u8; 8]).await {
                return;
            }
        }
        sender.finish();
    });

    match reader.read_to_end(Some(16)).await {
        Err(HttpParseError::BodyStream(msg)) => {
            assert!(msg.contains("16"), "unexpected message: {}", msg);
        }
        other => panic!("expected a size-limit error, got {:?}", other.map(|b| b.len())),
    }
}

// ---- 8. the reader is handed out exactly once -------------------------------

#[tokio::test]
async fn the_body_reader_can_be_taken_only_once() {
    let (_sender, stream) = HttpBodyAsStream::create(4, None);

    assert!(stream.get_body_reader().is_ok());

    match stream.get_body_reader() {
        Err(HttpParseError::BodyStream(msg)) => assert_eq!(msg, "Body reader is already taken"),
        _ => panic!("the second get_body_reader() must fail"),
    }
}

#[test]
fn an_empty_stream_has_no_reader() {
    let stream = HttpBodyAsStream::empty();
    assert_eq!(stream.get_content_length(), None);

    for _ in 0..2 {
        match stream.get_body_reader() {
            Err(HttpParseError::BodyStream(msg)) => {
                assert_eq!(msg, "Body stream is not available")
            }
            _ => panic!("empty() must never produce a reader"),
        }
    }
}

// ---- 9. empty chunks are not EOF --------------------------------------------

#[tokio::test]
async fn empty_chunks_never_reach_the_consumer() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let reader = stream.get_body_reader().unwrap();

    tokio::spawn(async move {
        assert!(sender.send_chunk(Vec::new()).await);
        assert!(sender.send_chunk(b"data".to_vec()).await);
        assert!(sender.send_chunk(Vec::new()).await);
        sender.finish();
    });

    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"data".to_vec()));
    assert_eq!(reader.get_next_chunk().await.unwrap(), None);
}

// ---- 10. the SAME model, used to SEND a body as a stream --------------------
//
// The roles simply invert: here the test plays the application filling the channel, and the
// `HttpRequestBody::Stream` it gets back is what a transport (fl-url) would drain.

struct Rnd;
impl RandomStringGenerator for Rnd {
    fn generate_random_string(_len: usize) -> String {
        "TESTBOUNDARY0001".to_string()
    }
}

/// Everything a transport does with the body it is handed.
fn take_stream(body: HttpRequestBody) -> HttpBodyAsStream {
    assert!(body.is_stream());
    // A stream carries no content type of its own — the model states it with a header field.
    assert!(body.get_content_type().is_none());
    match body {
        HttpRequestBody::Stream(stream) => stream,
        _ => panic!("expected HttpRequestBody::Stream"),
    }
}

#[tokio::test]
async fn a_streaming_model_builds_an_outgoing_request_that_streams() {
    let (sender, stream) = HttpBodyAsStream::create(4, Some(9));

    let model = UploadHttpInput {
        file_name: "report.bin".to_string(),
        overwrite: false,
        body: stream,
    };

    // url and headers are built exactly as for any other model
    let mut url = UrlBuilder::new("https://api.example.com");
    model.fill_url(&mut url).unwrap();
    assert_eq!(url.to_string(), "https://api.example.com?overwrite=false");

    // the application feeds the body — this is what the transport will pull out
    tokio::spawn(async move {
        for chunk in [b"aaa".to_vec(), b"bbb".to_vec(), b"ccc".to_vec()] {
            assert!(sender.send_chunk(chunk).await);
        }
        sender.finish();
    });

    // `get_body` consumes the model, so the stream MOVES out — no clone.
    let outgoing = take_stream(model.get_body::<Rnd>().unwrap());

    // the Content-Length the caller declared travels with it, both before and after taking a reader
    assert_eq!(outgoing.get_content_length(), Some(9));
    let reader = outgoing.get_body_reader().unwrap();
    assert_eq!(reader.get_content_length(), Some(9));

    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"aaa".to_vec()));
    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"bbb".to_vec()));
    assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"ccc".to_vec()));
    assert_eq!(reader.get_next_chunk().await.unwrap(), None);
}

#[tokio::test]
async fn an_outgoing_stream_that_breaks_off_is_an_error_for_the_transport_too() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);

    let model = UploadHttpInput {
        file_name: "report.bin".to_string(),
        overwrite: false,
        body: stream,
    };

    tokio::spawn(async move {
        assert!(sender.send_chunk(b"first".to_vec()).await);
        // the producer died mid-body — dropped WITHOUT finish()
    });

    let reader = take_stream(model.get_body::<Rnd>().unwrap())
        .get_body_reader()
        .unwrap();

    assert_eq!(
        reader.get_next_chunk().await.unwrap(),
        Some(b"first".to_vec())
    );

    match reader.get_next_chunk().await {
        Err(HttpParseError::BodyStream(msg)) => {
            assert_eq!(msg, "Request body stream ended unexpectedly");
        }
        other => panic!("a truncated upload must not look like a clean end: {:?}", other),
    }
}

#[test]
fn a_model_with_nothing_to_send_yields_a_stream_with_no_reader() {
    let model = UploadHttpInput {
        file_name: "report.bin".to_string(),
        overwrite: false,
        body: HttpBodyAsStream::empty(),
    };

    // Still a `Stream` — but the transport learns there is nothing to send from the reader.
    let outgoing = take_stream(model.get_body::<Rnd>().unwrap());
    assert_eq!(outgoing.get_content_length(), None);

    match outgoing.get_body_reader() {
        Err(HttpParseError::BodyStream(msg)) => assert_eq!(msg, "Body stream is not available"),
        _ => panic!("empty() must never produce a reader"),
    }
}

#[test]
fn other_body_kinds_are_not_streams() {
    // The guard a transport uses stays false for everything that does have bytes.
    assert!(!HttpRequestBody::Empty.is_stream());
    assert!(!HttpRequestBody::Json(b"{}".to_vec()).is_stream());
    assert!(!PlainBodyHttpInput {
        name: "John".to_string()
    }
    .get_body::<Rnd>()
    .unwrap()
    .is_stream());
}

// ---- 11. poll-based reading, for a transport that is itself a Body ----------

/// Drives `poll_next_chunk` the way `hyper::body::Body::poll_frame` would.
async fn poll_once(
    reader: &mut HttpBodyReader,
) -> Option<Result<Vec<u8>, HttpParseError>> {
    std::future::poll_fn(|cx| reader.poll_next_chunk(cx)).await
}

#[tokio::test]
async fn poll_next_chunk_delivers_the_chunks_then_a_clean_end() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let mut reader = stream.get_body_reader().unwrap();

    tokio::spawn(async move {
        for chunk in [b"aa".to_vec(), b"bb".to_vec()] {
            assert!(sender.send_chunk(chunk).await);
        }
        sender.finish();
    });

    assert_eq!(poll_once(&mut reader).await.unwrap().unwrap(), b"aa".to_vec());
    assert_eq!(poll_once(&mut reader).await.unwrap().unwrap(), b"bb".to_vec());
    assert!(poll_once(&mut reader).await.is_none());
}

#[tokio::test]
async fn poll_next_chunk_reports_an_abort_rather_than_an_end() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let mut reader = stream.get_body_reader().unwrap();

    tokio::spawn(async move {
        assert!(sender.send_chunk(b"first".to_vec()).await);
        // dropped WITHOUT finish()
    });

    assert_eq!(
        poll_once(&mut reader).await.unwrap().unwrap(),
        b"first".to_vec()
    );

    match poll_once(&mut reader).await {
        Some(Err(HttpParseError::BodyStream(msg))) => {
            assert_eq!(msg, "Request body stream ended unexpectedly");
        }
        other => panic!("expected an abort, got {:?}", other),
    }
}

#[tokio::test]
async fn poll_next_chunk_surfaces_a_sent_error() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let mut reader = stream.get_body_reader().unwrap();

    tokio::spawn(async move {
        sender
            .send_error(HttpParseError::BodyStream("disk read failed".to_string()))
            .await;
    });

    match poll_once(&mut reader).await {
        Some(Err(HttpParseError::BodyStream(msg))) => assert_eq!(msg, "disk read failed"),
        other => panic!("expected the producer's error, got {:?}", other),
    }
}

#[tokio::test]
async fn poll_next_chunk_is_pending_while_nothing_has_arrived() {
    let (sender, stream) = HttpBodyAsStream::create(4, None);
    let mut reader = stream.get_body_reader().unwrap();

    // Nothing sent yet: the poll must park, not report an end of body.
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    assert!(reader.poll_next_chunk(&mut cx).is_pending());

    assert!(sender.send_chunk(b"late".to_vec()).await);
    assert_eq!(
        poll_once(&mut reader).await.unwrap().unwrap(),
        b"late".to_vec()
    );
}

// ---- 12. Send + Sync --------------------------------------------------------
//
// Not optional: the model lives across `.await` inside a handler, and `HandleHttpRequest` in
// my-http-server-controllers is `#[async_trait]`, so that future MUST be `Send`.

fn _assert_send_sync<T: Send + Sync>() {}

fn _checks() {
    _assert_send_sync::<HttpBodyAsStream>();
    _assert_send_sync::<HttpBodyReader>();
    _assert_send_sync::<my_http_utils::http_input::HttpBodyStreamSender>();
    _assert_send_sync::<UploadHttpInput>();
}
