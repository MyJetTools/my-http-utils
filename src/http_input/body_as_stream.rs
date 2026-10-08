//! Carrying the request body **as a stream of chunks**, instead of materialising it whole.
//!
//! The channel is **bidirectional in role, not in direction**: bytes always flow sender → reader,
//! but which side is which depends on who is talking.
//!
//! * **Server** (`#[http_body_as_stream]` on an incoming request): my-http-server takes
//!   `hyper::body::Incoming`, loops over the frames and fills [`HttpBodyStreamSender`]; the
//!   handler takes the [`HttpBodyReader`] out of the parsed model and reads the chunks.
//! * **Client** (the same model used to *build* a request): application code calls
//!   [`HttpBodyAsStream::create`] and fills the sender itself; the transport (fl-url) takes the
//!   reader and writes the chunks into the socket (native) or into a `ReadableStream` for `fetch`
//!   (wasm).
//!
//! This module knows nothing about either transport — no hyper, no `tokio::spawn`, no sockets. Its
//! only dependency is `tokio::sync` (mpsc + `Mutex`), which is platform-independent.
//!
//! ```ignore
//! #[derive(MyHttpInput)]
//! pub struct UploadHttpInput {
//!     #[http_header(name = "X-File-Name", description = "File name")]
//!     pub file_name: String,
//!     #[http_body_as_stream(description = "File content")]
//!     pub body: HttpBodyAsStream,
//! }
//!
//! // server, in the handler — the transport already filled the channel:
//! let reader = input_data.body.get_body_reader()?;
//! let expected = reader.get_content_length();   // Some(n) with Content-Length, None when chunked
//! while let Some(chunk) = reader.get_next_chunk().await? {
//!     // chunk: Vec<u8>
//! }
//!
//! // client — the roles invert: we fill the channel, the transport reads it.
//! let (sender, stream) = HttpBodyAsStream::create(BODY_STREAM_DEFAULT_BUFFER, Some(total_len));
//! tokio::spawn(async move {
//!     while let Some(chunk) = next_chunk_from_disk().await {
//!         if !sender.send_chunk(chunk).await { return; }   // the transport gave up
//!     }
//!     sender.finish();
//! });
//! let model = UploadHttpInput { file_name: "report.bin".into(), body: stream };
//! // model.get_body::<Rnd>() -> HttpRequestBody::Stream(..), which fl-url writes out chunk by chunk
//! ```
//!
//! **A stream instead of the channel.** When the bytes already come as a
//! [`rust_extensions::AsyncBytesStream`] — a response body read through fl-url / my-http-client, an
//! incoming body the server wraps — [`HttpBodyAsStream::from_bytes_stream`] hands that stream to
//! the reader as it is: no channel, no pump to spawn. The [`HttpBodyReader`] reads it the same way
//! it reads the channel, and nobody downstream can tell the two apart.
//!
//! **Feature gating.** None of this is gated: both sides need the channel, so a wasm client that
//! builds *without* `server` gets the whole thing. The one `server`-only item is the
//! `DataTypeProvider` impl at the bottom, which needs the OpenAPI `schema` module.

use crate::http_input::HttpParseError;

use rust_extensions::AsyncBytesStream;

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{ready, Context, Poll};

/// Default capacity of the chunk channel: how many chunks the pump may read ahead of the handler.
///
/// The channel is **bounded**, so this is also the memory ceiling per request:
/// roughly `BODY_STREAM_DEFAULT_BUFFER × chunk_size`.
pub const BODY_STREAM_DEFAULT_BUFFER: usize = 4;

/// Chunks of a request body — where they come from (the receiving half of the channel, or an
/// [`AsyncBytesStream`]), plus the length when it is known.
///
/// On the **server** it is produced by `parse` for a `#[http_body_as_stream]` field and the
/// transport has already started filling it. On the **client** the application creates it with
/// [`create`](Self::create) (or [`from_bytes_stream`](Self::from_bytes_stream)), puts it into the
/// model field, and the transport takes the reader out of the resulting `HttpRequestBody::Stream`.
///
/// Either way the reader comes out exactly once, via [`get_body_reader`](Self::get_body_reader).
pub struct HttpBodyAsStream {
    inner: std::sync::Mutex<BodyStreamState>,
    content_length: Option<u64>,
}

/// Where the chunks come from, until the reader takes them.
///
/// A three-state enum rather than a plain `Option`, because the two failure modes of
/// [`HttpBodyAsStream::get_body_reader`] must be told apart: *there never was a stream*
/// (an [`HttpBodyAsStream::empty`]) vs *the reader has already been taken*.
enum BodyStreamState {
    /// No stream at all — [`HttpBodyAsStream::empty`]: there is nothing to send or receive.
    NotAvailable,
    /// The source of the chunks, not taken yet.
    Ready(BodySource),
    /// [`HttpBodyAsStream::get_body_reader`] has already handed the reader out. There is exactly
    /// one source, so there is no second one to give.
    Taken,
}

/// Where the reader gets its chunks from.
enum BodySource {
    /// The channel [`HttpBodyAsStream::create`] makes, filled through [`HttpBodyStreamSender`].
    Channel(ChannelSource),
    /// A stream that is read directly — [`HttpBodyAsStream::from_bytes_stream`].
    Stream(BytesStreamSource),
}

impl BodySource {
    /// The next chunk; `Ok(None)` is the end of the body. Both
    /// [`HttpBodyReader::get_next_chunk`] and [`HttpBodyReader::poll_next_chunk`] come down to
    /// this, so the two can not read a body differently.
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<Vec<u8>>, HttpParseError>> {
        match self {
            Self::Channel(channel) => channel.poll_next(cx),
            Self::Stream(stream) => stream.poll_next(cx),
        }
    }
}

/// The receiving half plus the "did the body actually finish?" flag.
struct ChannelSource {
    rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, HttpParseError>>,
    completed: Arc<AtomicBool>,
}

impl ChannelSource {
    /// `Receiver::poll_recv` gives `None` both when the body ended normally *and* when every
    /// sender was dropped because the pump died. Treating the second case as EOF would silently
    /// truncate the body — data corruption that looks like success. Hence the `completed` flag,
    /// which only [`HttpBodyStreamSender::finish`] sets: a channel that closed without it is an
    /// abort, and it is reported as [`HttpParseError::BodyStream`].
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<Vec<u8>>, HttpParseError>> {
        match ready!(self.rx.poll_recv(cx)) {
            Some(chunk) => Poll::Ready(chunk.map(Some)),
            None if self.completed.load(Ordering::Acquire) => Poll::Ready(Ok(None)),
            None => Poll::Ready(Err(HttpParseError::BodyStream(
                "Request body stream ended unexpectedly".to_string(),
            ))),
        }
    }
}

/// Reading the next chunk of an [`AsyncBytesStream`] — a future that owns the stream it reads, so
/// it can be kept from one poll to the next.
type NextChunk = Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, HttpParseError>> + Send>>;

/// An [`AsyncBytesStream`] with its error and chunk types taken off, so that
/// [`HttpBodyAsStream`] stays one type whatever stream it reads.
trait ErasedBytesStream: Send + Sync {
    /// Takes the stream by `Arc`: the future owns what it reads, so the read in progress can sit
    /// next to the stream in [`BytesStreamSource`] without borrowing it.
    fn next_chunk(self: Arc<Self>) -> NextChunk;
}

struct TypedBytesStream<TStream, TError> {
    stream: TStream,
    // `TError` is only what `get_next()` fails with — none of it is kept
    error: PhantomData<fn() -> TError>,
}

impl<TStream, TError> ErasedBytesStream for TypedBytesStream<TStream, TError>
where
    TStream: AsyncBytesStream<TError> + Send + Sync + 'static,
    TError: Into<HttpParseError> + 'static,
{
    fn next_chunk(self: Arc<Self>) -> NextChunk {
        Box::pin(async move {
            loop {
                match self.stream.get_next().await {
                    // Passed by, just like `HttpBodyStreamSender::send_chunk` drops it: an empty
                    // chunk must never be mistaken for "almost the end"
                    Ok(Some(chunk)) if chunk.is_empty() => {}
                    // Copied out and let go at once, as `AsyncBytesStream` asks of its readers -
                    // a stream that reads into buffers of its own gets them back right away
                    Ok(Some(chunk)) => return Ok(Some(chunk.to_vec())),
                    Ok(None) => return Ok(None),
                    Err(err) => return Err(err.into()),
                }
            }
        })
    }
}

struct BytesStreamSource {
    stream: Arc<dyn ErasedBytesStream>,
    /// The read in progress. It is kept between polls, so a read that is given up half-way (the
    /// `get_next_chunk` future dropped) is picked up by the next call instead of losing a chunk.
    reading: Option<NextChunk>,
    /// The stream said where its end is — it is not asked again.
    ended: bool,
}

impl BytesStreamSource {
    fn new<TStream, TError>(stream: TStream) -> Self
    where
        TStream: AsyncBytesStream<TError> + Send + Sync + 'static,
        TError: Into<HttpParseError> + 'static,
    {
        Self {
            stream: Arc::new(TypedBytesStream {
                stream,
                error: PhantomData,
            }),
            reading: None,
            ended: false,
        }
    }

    /// The stream says where the body ends: `Ok(None)` out of it is the end, and a body that is
    /// cut short has to come out of it as an error. Its error is handed over as it is (via
    /// `Into<HttpParseError>`), and the next call asks the stream again — as `AsyncBytesStream`
    /// has it.
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<Vec<u8>>, HttpParseError>> {
        if self.ended {
            return Poll::Ready(Ok(None));
        }

        let stream = &self.stream;
        let reading = self
            .reading
            .get_or_insert_with(|| stream.clone().next_chunk());

        let result = ready!(reading.as_mut().poll(cx));
        self.reading = None;

        if let Ok(None) = result {
            self.ended = true;
        }

        Poll::Ready(result)
    }
}

impl HttpBodyAsStream {
    /// There is nothing to stream. Every [`get_body_reader`](Self::get_body_reader) on it fails
    /// with `"Body stream is not available"` — which is exactly how a transport tells a model that
    /// carries no body from one that does.
    pub fn empty() -> Self {
        Self {
            inner: std::sync::Mutex::new(BodyStreamState::NotAvailable),
            content_length: None,
        }
    }

    /// Creates the pair «filler + model field» — the sending half and the stream that goes into
    /// the model. Called by the server's transport for an incoming body, and by application code
    /// for an outgoing one.
    ///
    /// `buffer` is the capacity of the **bounded** channel — the back-pressure knob. A pump that
    /// runs into a full channel parks on `send().await`, and that pressure propagates all the way
    /// down to the TCP window; an unbounded channel would instead let a fast producer eat memory.
    /// See [`BODY_STREAM_DEFAULT_BUFFER`]. `0` is treated as `1` (tokio's channel rejects `0`).
    ///
    /// `content_length` is the total body length when it is known up front — the transport turns
    /// it into a `Content-Length` header; `None` means a chunked body.
    pub fn create(
        buffer: usize,
        content_length: Option<u64>,
    ) -> (HttpBodyStreamSender, HttpBodyAsStream) {
        let (tx, rx) = tokio::sync::mpsc::channel(if buffer == 0 { 1 } else { buffer });

        let completed = Arc::new(AtomicBool::new(false));

        let sender = HttpBodyStreamSender {
            tx,
            completed: completed.clone(),
        };

        let stream = HttpBodyAsStream {
            inner: std::sync::Mutex::new(BodyStreamState::Ready(BodySource::Channel(
                ChannelSource { rx, completed },
            ))),
            content_length,
        };

        (sender, stream)
    }

    /// The model field over a stream that is already there — anything that is an
    /// [`AsyncBytesStream`]: a body the server reads off the connection, a response body read
    /// through fl-url / my-http-client (proxying it as an outgoing request body), a file. This is
    /// what the derive-generated `parse_with_body_stream` puts into a `#[http_body_as_stream]`
    /// field.
    ///
    /// There is no channel and nothing to spawn: the [`HttpBodyReader`] reads the stream itself, a
    /// chunk when it is asked for one, so the back-pressure is the stream's own.
    ///
    /// The length is what the stream says of itself ([`AsyncBytesStream::get_size`]) — a
    /// transport sends it as `Content-Length`; `None` means a chunked body. The end of the body is
    /// what the stream says as well: `Ok(None)` out of it is the end, so a body that is cut short
    /// has to come out of it as an error (its error becomes an [`HttpParseError`] via `Into`).
    pub fn from_bytes_stream<TStream, TError>(stream: TStream) -> Self
    where
        TStream: AsyncBytesStream<TError> + Send + Sync + 'static,
        TError: Into<HttpParseError> + 'static,
    {
        let content_length = stream.get_size().map(|size| size as u64);

        Self {
            inner: std::sync::Mutex::new(BodyStreamState::Ready(BodySource::Stream(
                BytesStreamSource::new(stream),
            ))),
            content_length,
        }
    }

    /// Takes the reader out. The first call hands it over, every next one fails: there is exactly
    /// one receiving half of the channel.
    ///
    /// Takes `&self` — `parse` puts the model together behind a shared reference, and both the
    /// server handler and the client transport read the body out of an already-built value. The
    /// take happens under a **synchronous** `Mutex` whose guard never crosses an `.await` (it is
    /// dropped before this function returns anything), so the caller's future stays `Send`.
    pub fn get_body_reader(&self) -> Result<HttpBodyReader, HttpParseError> {
        let taken = {
            // A poisoned lock here would mean a panic inside the few lines below, which do not
            // panic. Recover rather than propagate a panic into request handling.
            let mut lock = self.inner.lock().unwrap_or_else(|err| err.into_inner());

            match &mut *lock {
                BodyStreamState::Ready(..) => {
                    match std::mem::replace(&mut *lock, BodyStreamState::Taken) {
                        BodyStreamState::Ready(source) => Ok(source),
                        _ => unreachable!(),
                    }
                }
                // Not "taken" — it never existed. Leave the state alone so the message stays
                // truthful on every repeated call.
                BodyStreamState::NotAvailable => Err("Body stream is not available"),
                BodyStreamState::Taken => Err("Body reader is already taken"),
            }
            // The guard is dropped here — it never crosses an `.await`, so the caller's future
            // stays `Send`.
        };

        match taken {
            Ok(source) => Ok(HttpBodyReader {
                source: tokio::sync::Mutex::new(source),
                content_length: self.content_length,
            }),
            Err(msg) => Err(HttpParseError::BodyStream(msg.to_string())),
        }
    }

    /// The body length when it is known up front (`Content-Length`). `None` for a chunked body.
    pub fn get_content_length(&self) -> Option<u64> {
        self.content_length
    }
}

/// The sending half — whoever produces the bytes holds it: my-http-server pouring hyper frames
/// into an incoming body, or application code feeding an outgoing one.
pub struct HttpBodyStreamSender {
    tx: tokio::sync::mpsc::Sender<Result<Vec<u8>, HttpParseError>>,
    completed: Arc<AtomicBool>,
}

impl HttpBodyStreamSender {
    /// Hands over the next chunk. `false` means **the reader is gone** (dropped) — the pump must
    /// stop immediately.
    ///
    /// Empty chunks are dropped right here, so that `Ok(Some(vec![]))` never reaches the consumer
    /// and can never be mistaken for "almost the end" — an empty chunk is not EOF.
    ///
    /// Awaits while the channel is full: that wait *is* the back-pressure.
    pub async fn send_chunk(&self, chunk: Vec<u8>) -> bool {
        if chunk.is_empty() {
            // Nothing to deliver, but still report whether it is worth going on.
            return !self.tx.is_closed();
        }

        self.tx.send(Ok(chunk)).await.is_ok()
    }

    /// Reports a read failure. The pump must stop after this — the reader gets the error out of
    /// its next [`HttpBodyReader::get_next_chunk`].
    pub async fn send_error(&self, err: HttpParseError) {
        let _ = self.tx.send(Err(err)).await;
    }

    /// Resolves when the reader is gone (the [`HttpBodyReader`] was dropped).
    ///
    /// Not decoration: without it, a pump asleep waiting for data from a client that went quiet
    /// would never learn that the reader left, and would hang forever. `send_chunk` only reports
    /// it on the *next* chunk — which may never come.
    pub async fn closed(&self) {
        self.tx.closed().await
    }

    /// Marks the body as delivered **in full and normally**. Call it right before dropping the
    /// sender.
    ///
    /// Without it, a dropped sender is indistinguishable from a pump that died half-way, and the
    /// reader reports the truncation instead of a clean EOF — see [`HttpBodyReader::get_next_chunk`].
    pub fn finish(&self) {
        // `Release` pairs with the `Acquire` load in the reader, so the flag is guaranteed visible
        // there without relying on the channel's own internal ordering.
        self.completed.store(true, Ordering::Release);
    }
}

/// The reading half, handed out by [`HttpBodyAsStream::get_body_reader`] — to the server handler
/// for an incoming body, to the transport for an outgoing one.
///
/// [`get_next_chunk`](Self::get_next_chunk) and [`read_to_end`](Self::read_to_end) take `&self`
/// (the source sits behind a `tokio::sync::Mutex`), so the reader can be put into an `Arc` and
/// read from several places. A transport that is itself a `Future`/`Body` owns the reader outright
/// and uses [`poll_next_chunk`](Self::poll_next_chunk) instead.
///
/// It is an [`AsyncBytesStream`] as well, so whatever reads a stream of bytes reads the body — my-json's
/// `JsonArrayIteratorAsync` taking a huge JSON array apart as it arrives, or fl-url sending it on.
pub struct HttpBodyReader {
    source: tokio::sync::Mutex<BodySource>,
    content_length: Option<u64>,
}

impl HttpBodyReader {
    /// The next chunk of the body. `Ok(None)` means the body has been read **in full**.
    ///
    /// The distinction matters. Out of the channel ([`HttpBodyAsStream::create`]) a closed
    /// channel is an end of body only when the producer called [`HttpBodyStreamSender::finish`];
    /// one that closed without it is an abort, reported as [`HttpParseError::BodyStream`] — never
    /// a silent truncation. Out of a stream ([`HttpBodyAsStream::from_bytes_stream`]) the stream
    /// itself says where its end is.
    ///
    /// Cancel-safe: a read that is given up half-way (this future dropped) loses nothing — the
    /// next call picks it up.
    pub async fn get_next_chunk(&self) -> Result<Option<Vec<u8>>, HttpParseError> {
        let mut source = self.source.lock().await;
        std::future::poll_fn(|cx| source.poll_next(cx)).await
    }

    /// Polls the next chunk, for a transport that is itself a `Future` / `Body` — fl-url's
    /// `hyper::body::Body::poll_frame` on native, where the caller has a `Context` and no place to
    /// `.await`.
    ///
    /// Takes `&mut self`, so the `Mutex` is reached through `get_mut()` and never actually locked
    /// — no lock contention. That is the trade against [`get_next_chunk`](Self::get_next_chunk):
    /// exclusive ownership instead of `Arc`-sharing.
    ///
    /// Semantics are identical to [`get_next_chunk`](Self::get_next_chunk) — both come down to
    /// the same code: `Poll::Ready(None)` means the body arrived **in full**, and a channel closed
    /// without [`HttpBodyStreamSender::finish`] is an abort reported as
    /// [`HttpParseError::BodyStream`] — never a silent EOF.
    pub fn poll_next_chunk(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Vec<u8>, HttpParseError>>> {
        // `get_mut` — a `&mut self` proves nobody else holds the mutex, so there is nothing to lock.
        self.source.get_mut().poll_next(cx).map(Result::transpose)
    }

    /// The body length when it is known up front (`Content-Length`). `None` for a chunked body.
    pub fn get_content_length(&self) -> Option<u64> {
        self.content_length
    }

    /// Reads the rest of the body into memory. `max_size` is the safety valve — going over it
    /// gives [`HttpParseError::BodyStream`] instead of an unbounded allocation. `None` means no
    /// limit, so only pass it where the source is trusted.
    ///
    /// This is also how the transport layer can implement "just give me the whole body".
    pub async fn read_to_end(&self, max_size: Option<usize>) -> Result<Vec<u8>, HttpParseError> {
        let mut result: Vec<u8> = Vec::new();

        while let Some(chunk) = self.get_next_chunk().await? {
            if let Some(max_size) = max_size {
                if result.len() + chunk.len() > max_size {
                    return Err(HttpParseError::BodyStream(format!(
                        "Request body is bigger than the allowed {} bytes",
                        max_size
                    )));
                }
            }

            if result.is_empty() {
                // The common single-chunk case moves the buffer instead of copying it.
                result = chunk;
            } else {
                result.extend_from_slice(&chunk);
            }
        }

        Ok(result)
    }
}

/// The body as a stream of bytes — the same chunks [`HttpBodyReader::get_next_chunk`] gives, with
/// the same "a cut-short body is an error, not an end" rule.
///
/// The `into_vec()` the trait comes with is [`HttpBodyReader::read_to_end`] with no limit; where
/// the source is not trusted, call `read_to_end` with one.
#[async_trait::async_trait]
impl AsyncBytesStream<HttpParseError> for HttpBodyReader {
    type Chunk = Vec<u8>;

    async fn get_next(&self) -> Result<Option<Vec<u8>>, HttpParseError> {
        self.get_next_chunk().await
    }

    /// The `Content-Length` — see [`HttpBodyReader::get_content_length`].
    fn get_size(&self) -> Option<usize> {
        self.content_length.map(|size| size as usize)
    }

    async fn into_vec(&self) -> Result<Vec<u8>, HttpParseError> {
        // Not the default of the trait: that one allocates the announced size up front, and an
        // announced size is whatever the other side wrote into a header
        self.read_to_end(None).await
    }
}

/// Schema (server-only), same as [`crate::http_input::RawData`]: a streamed body carries no inner
/// model, so OpenAPI describes it as `binary` (`type: string, format: binary`). Without this the
/// derive's `#field_type::get_data_type()` would not resolve and a `#[http_body_as_stream]` model
/// would not compile with the schema on.
#[cfg(feature = "server")]
impl crate::schema::data_types::DataTypeProvider for HttpBodyAsStream {
    fn get_data_type() -> crate::schema::data_types::HttpDataType {
        crate::schema::data_types::HttpDataType::SimpleType(
            crate::schema::data_types::HttpSimpleType::Binary,
        )
    }
}

/// The **client** half of the contract, exercised with **no features on** — i.e. exactly what a
/// wasm/browser build compiles. The `tests` workspace member always enables `server`, so it can
/// never prove this; a plain `cargo test` runs the module below.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::body::HttpRequestBody;
    use crate::schema::client::{RandomStringGenerator, THttpRequestBuilder};

    #[derive(crate::macros::MyHttpInput)]
    struct UploadHttpInput {
        #[http_header(name = "X-File-Name", description = "File name")]
        file_name: String,
        #[http_body_as_stream(description = "File content")]
        body: HttpBodyAsStream,
    }

    struct Rnd;
    impl RandomStringGenerator for Rnd {
        fn generate_random_string(_len: usize) -> String {
            "TESTBOUNDARY0001".to_string()
        }
    }

    fn take_stream(body: HttpRequestBody) -> HttpBodyAsStream {
        assert!(body.is_stream());
        assert!(body.get_content_type().is_none());
        match body {
            HttpRequestBody::Stream(stream) => stream,
            _ => panic!("expected HttpRequestBody::Stream"),
        }
    }

    #[tokio::test]
    async fn a_client_streams_an_outgoing_body_without_the_server_feature() {
        let (sender, stream) = HttpBodyAsStream::create(BODY_STREAM_DEFAULT_BUFFER, Some(6));

        let model = UploadHttpInput {
            file_name: "report.bin".to_string(),
            body: stream,
        };

        tokio::spawn(async move {
            for chunk in [b"aaa".to_vec(), b"bbb".to_vec()] {
                assert!(sender.send_chunk(chunk).await);
            }
            sender.finish();
        });

        // What a transport (fl-url) does with the model.
        let outgoing = take_stream(model.get_body::<Rnd>().unwrap());
        assert_eq!(outgoing.get_content_length(), Some(6));

        let reader = outgoing.get_body_reader().unwrap();
        assert_eq!(reader.get_content_length(), Some(6));
        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"aaa".to_vec()));
        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"bbb".to_vec()));
        assert_eq!(reader.get_next_chunk().await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_truncated_outgoing_body_is_an_error_without_the_server_feature() {
        let (sender, stream) = HttpBodyAsStream::create(4, None);
        let reader = stream.get_body_reader().unwrap();

        tokio::spawn(async move {
            assert!(sender.send_chunk(b"first".to_vec()).await);
            // dropped WITHOUT finish()
        });

        assert_eq!(
            reader.get_next_chunk().await.unwrap(),
            Some(b"first".to_vec())
        );
        assert!(matches!(
            reader.get_next_chunk().await,
            Err(HttpParseError::BodyStream(_))
        ));
    }

    #[tokio::test]
    async fn poll_next_chunk_works_without_the_server_feature() {
        let (sender, stream) = HttpBodyAsStream::create(4, None);
        let mut reader = stream.get_body_reader().unwrap();

        // Nothing sent yet — park, do not report an end of body.
        let waker = std::task::Waker::noop();
        assert!(reader
            .poll_next_chunk(&mut std::task::Context::from_waker(waker))
            .is_pending());

        tokio::spawn(async move {
            assert!(sender.send_chunk(b"data".to_vec()).await);
            sender.finish();
        });

        let chunk = std::future::poll_fn(|cx| reader.poll_next_chunk(cx))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(chunk, b"data".to_vec());
        assert!(std::future::poll_fn(|cx| reader.poll_next_chunk(cx))
            .await
            .is_none());
    }

    // ---- a stream instead of the channel ----------------------------------------------------

    use std::collections::VecDeque;
    use std::sync::atomic::AtomicUsize;

    /// A stream that is already there — what fl-url's response body reader is. Counts how many
    /// times it is asked.
    struct Chunks {
        chunks: std::sync::Mutex<VecDeque<Result<Vec<u8>, HttpParseError>>>,
        size: Option<usize>,
        requests: Arc<AtomicUsize>,
    }

    impl Chunks {
        fn new(chunks: Vec<Result<&[u8], HttpParseError>>, size: Option<usize>) -> Self {
            Self {
                chunks: std::sync::Mutex::new(
                    chunks
                        .into_iter()
                        .map(|chunk| chunk.map(|chunk| chunk.to_vec()))
                        .collect(),
                ),
                size,
                requests: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait::async_trait]
    impl AsyncBytesStream<HttpParseError> for Chunks {
        type Chunk = Vec<u8>;

        async fn get_next(&self) -> Result<Option<Vec<u8>>, HttpParseError> {
            self.requests.fetch_add(1, Ordering::Relaxed);
            self.chunks.lock().unwrap().pop_front().transpose()
        }

        fn get_size(&self) -> Option<usize> {
            self.size
        }
    }

    #[tokio::test]
    async fn a_client_streams_a_stream_it_already_has() {
        let stream = Chunks::new(vec![Ok(b"aaa"), Ok(b"bbb")], Some(6));
        let requests = stream.requests.clone();

        let model = UploadHttpInput {
            file_name: "report.bin".to_string(),
            body: HttpBodyAsStream::from_bytes_stream(stream),
        };

        // What a transport (fl-url) does with the model — no pump anywhere
        let outgoing = take_stream(model.get_body::<Rnd>().unwrap());
        assert_eq!(outgoing.get_content_length(), Some(6));

        let reader = outgoing.get_body_reader().unwrap();
        assert_eq!(reader.get_content_length(), Some(6));
        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"aaa".to_vec()));
        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"bbb".to_vec()));
        assert_eq!(reader.get_next_chunk().await.unwrap(), None);

        // Past the end the stream is not asked again
        assert_eq!(reader.get_next_chunk().await.unwrap(), None);
        assert_eq!(requests.load(Ordering::Relaxed), 3);

        // Only one reader, as with the channel
        assert!(outgoing.get_body_reader().is_err());
    }

    #[tokio::test]
    async fn empty_chunks_of_a_stream_never_reach_the_reader() {
        let stream = Chunks::new(vec![Ok(b""), Ok(b"a"), Ok(b""), Ok(b""), Ok(b"b")], None);
        let reader = HttpBodyAsStream::from_bytes_stream(stream)
            .get_body_reader()
            .unwrap();

        assert_eq!(reader.get_content_length(), None);
        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"a".to_vec()));
        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"b".to_vec()));
        assert_eq!(reader.get_next_chunk().await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_error_of_a_stream_is_handed_over_and_the_stream_is_asked_again() {
        let broken = HttpParseError::BodyStream("connection reset".to_string());
        let stream = Chunks::new(vec![Ok(b"a"), Err(broken.clone()), Ok(b"b")], None);
        let reader = HttpBodyAsStream::from_bytes_stream(stream)
            .get_body_reader()
            .unwrap();

        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"a".to_vec()));
        assert_eq!(reader.get_next_chunk().await, Err(broken));
        // As `AsyncBytesStream` has it: whether to go on after an error is the stream's to say
        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"b".to_vec()));
        assert_eq!(reader.get_next_chunk().await.unwrap(), None);
    }

    #[tokio::test]
    async fn poll_next_chunk_reads_a_stream_too() {
        let stream = Chunks::new(vec![Ok(b"data")], None);
        let mut reader = HttpBodyAsStream::from_bytes_stream(stream)
            .get_body_reader()
            .unwrap();

        let chunk = std::future::poll_fn(|cx| reader.poll_next_chunk(cx))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(chunk, b"data".to_vec());
        assert!(std::future::poll_fn(|cx| reader.poll_next_chunk(cx))
            .await
            .is_none());
    }

    /// Gives a chunk only once a permit is added — a stream that has nothing to give yet.
    struct Gated {
        gate: tokio::sync::Semaphore,
        chunks: std::sync::Mutex<VecDeque<Vec<u8>>>,
        requests: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl AsyncBytesStream<HttpParseError> for Gated {
        type Chunk = Vec<u8>;

        async fn get_next(&self) -> Result<Option<Vec<u8>>, HttpParseError> {
            self.requests.fetch_add(1, Ordering::Relaxed);
            self.gate.acquire().await.unwrap().forget();
            Ok(self.chunks.lock().unwrap().pop_front())
        }

        fn get_size(&self) -> Option<usize> {
            None
        }
    }

    #[tokio::test]
    async fn a_read_given_up_half_way_loses_no_chunk() {
        let gated = Arc::new(Gated {
            gate: tokio::sync::Semaphore::new(0),
            chunks: std::sync::Mutex::new(VecDeque::from([b"a".to_vec()])),
            requests: AtomicUsize::new(0),
        });

        // `Arc<Gated>` is a stream as well, so the test keeps a hand on the gate
        let reader = HttpBodyAsStream::from_bytes_stream(gated.clone())
            .get_body_reader()
            .unwrap();

        {
            let mut read = std::pin::pin!(reader.get_next_chunk());
            let polled = std::future::poll_fn(|cx| Poll::Ready(read.as_mut().poll(cx))).await;
            assert!(polled.is_pending());
            // ...and given up: `read` is dropped here, in the middle of the read
        }

        gated.gate.add_permits(1);

        assert_eq!(reader.get_next_chunk().await.unwrap(), Some(b"a".to_vec()));
        // The read that was given up was picked up again — not started anew
        assert_eq!(gated.requests.load(Ordering::Relaxed), 1);
    }

    /// A chunk that holds a buffer of the stream, and says when it is let go.
    struct BorrowedBuffer {
        bytes: Vec<u8>,
        alive: Arc<AtomicUsize>,
    }

    impl std::ops::Deref for BorrowedBuffer {
        type Target = [u8];

        fn deref(&self) -> &[u8] {
            &self.bytes
        }
    }

    impl Drop for BorrowedBuffer {
        fn drop(&mut self) {
            self.alive.fetch_sub(1, Ordering::Relaxed);
        }
    }

    struct Buffers {
        chunks: std::sync::Mutex<VecDeque<Vec<u8>>>,
        alive: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl AsyncBytesStream<HttpParseError> for Buffers {
        type Chunk = BorrowedBuffer;

        async fn get_next(&self) -> Result<Option<BorrowedBuffer>, HttpParseError> {
            let chunk = self.chunks.lock().unwrap().pop_front();

            Ok(chunk.map(|bytes| {
                self.alive.fetch_add(1, Ordering::Relaxed);
                BorrowedBuffer {
                    bytes,
                    alive: self.alive.clone(),
                }
            }))
        }

        fn get_size(&self) -> Option<usize> {
            None
        }
    }

    #[tokio::test]
    async fn a_chunk_is_let_go_as_soon_as_it_is_copied() {
        let alive = Arc::new(AtomicUsize::new(0));
        let stream = Buffers {
            chunks: std::sync::Mutex::new(VecDeque::from([b"abc".to_vec()])),
            alive: alive.clone(),
        };
        let reader = HttpBodyAsStream::from_bytes_stream(stream)
            .get_body_reader()
            .unwrap();

        let chunk = reader.get_next_chunk().await.unwrap().unwrap();

        assert_eq!(chunk, b"abc".to_vec());
        // The stream has its buffer back while the reader's caller still holds the bytes
        assert_eq!(alive.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn the_reader_is_a_stream_of_bytes_whatever_feeds_it() {
        // Over the channel...
        let (sender, stream) = HttpBodyAsStream::create(4, Some(6));
        tokio::spawn(async move {
            assert!(sender.send_chunk(b"aaa".to_vec()).await);
            assert!(sender.send_chunk(b"bbb".to_vec()).await);
            sender.finish();
        });

        let reader = stream.get_body_reader().unwrap();
        assert_eq!(AsyncBytesStream::get_size(&reader), Some(6));
        assert_eq!(
            AsyncBytesStream::get_next(&reader).await.unwrap(),
            Some(b"aaa".to_vec())
        );
        assert_eq!(
            AsyncBytesStream::into_vec(&reader).await.unwrap(),
            b"bbb".to_vec()
        );

        // ...and over a stream, handed on as a trait object
        let stream = Chunks::new(vec![Ok(b"x"), Ok(b"yz")], Some(3));
        let reader: Arc<dyn AsyncBytesStream<HttpParseError, Chunk = Vec<u8>> + Send + Sync> =
            Arc::new(
                HttpBodyAsStream::from_bytes_stream(stream)
                    .get_body_reader()
                    .unwrap(),
            );
        assert_eq!(reader.get_size(), Some(3));
        assert_eq!(reader.into_vec().await.unwrap(), b"xyz".to_vec());
    }

    #[tokio::test]
    async fn a_model_with_nothing_to_send_still_yields_a_stream() {
        let model = UploadHttpInput {
            file_name: "report.bin".to_string(),
            body: HttpBodyAsStream::empty(),
        };

        let outgoing = take_stream(model.get_body::<Rnd>().unwrap());
        assert_eq!(outgoing.get_content_length(), None);

        match outgoing.get_body_reader() {
            Err(HttpParseError::BodyStream(msg)) => {
                assert_eq!(msg, "Body stream is not available")
            }
            _ => panic!("empty() must never produce a reader"),
        }
    }
}
