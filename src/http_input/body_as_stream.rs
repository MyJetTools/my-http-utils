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
//! **Feature gating.** None of this is gated: both sides need the channel, so a wasm client that
//! builds *without* `server` gets the whole thing. The one `server`-only item is the
//! `DataTypeProvider` impl at the bottom, which needs the OpenAPI `schema` module.

use crate::http_input::HttpParseError;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Default capacity of the chunk channel: how many chunks the pump may read ahead of the handler.
///
/// The channel is **bounded**, so this is also the memory ceiling per request:
/// roughly `BODY_STREAM_DEFAULT_BUFFER × chunk_size`.
pub const BODY_STREAM_DEFAULT_BUFFER: usize = 4;

/// Chunks of a request body — the receiving half of the channel, plus the length when it is known.
///
/// On the **server** it is produced by `parse` for a `#[http_body_as_stream]` field and the
/// transport has already started filling it. On the **client** the application creates it with
/// [`create`](Self::create), puts it into the model field, and the transport takes the reader out
/// of the resulting `HttpRequestBody::Stream`.
///
/// Either way the reader comes out exactly once, via [`get_body_reader`](Self::get_body_reader).
pub struct HttpBodyAsStream {
    inner: std::sync::Mutex<BodyStreamState>,
    content_length: Option<u64>,
}

/// The receiving half plus the "did the body actually finish?" flag.
///
/// A three-state enum rather than a plain `Option`, because the two failure modes of
/// [`HttpBodyAsStream::get_body_reader`] must be told apart: *there never was a stream*
/// (an [`HttpBodyAsStream::empty`]) vs *the reader has already been taken*.
enum BodyStreamState {
    /// No stream at all — [`HttpBodyAsStream::empty`]: there is nothing to send or receive.
    NotAvailable,
    /// The receiving half, not taken yet.
    Ready(BodyStreamInner),
    /// [`HttpBodyAsStream::get_body_reader`] has already handed the reader out. There is exactly
    /// one receiver, so there is no second one to give.
    Taken,
}

struct BodyStreamInner {
    rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, HttpParseError>>,
    completed: Arc<AtomicBool>,
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
            inner: std::sync::Mutex::new(BodyStreamState::Ready(BodyStreamInner { rx, completed })),
            content_length,
        };

        (sender, stream)
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
                        BodyStreamState::Ready(inner) => Ok(inner),
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
            Ok(inner) => Ok(HttpBodyReader {
                rx: tokio::sync::Mutex::new(inner.rx),
                completed: inner.completed,
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
/// (the receiver sits behind a `tokio::sync::Mutex`), so the reader can be put into an `Arc` and
/// read from several places. A transport that is itself a `Future`/`Body` owns the reader outright
/// and uses [`poll_next_chunk`](Self::poll_next_chunk) instead.
pub struct HttpBodyReader {
    rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Result<Vec<u8>, HttpParseError>>>,
    completed: Arc<AtomicBool>,
    content_length: Option<u64>,
}

impl HttpBodyReader {
    /// The next chunk of the body. `Ok(None)` means the body has been read **in full**.
    ///
    /// The distinction matters: `Receiver::recv()` returns `None` both when the body ended normally
    /// *and* when every sender was dropped because the pump died. Treating the second case as EOF
    /// would silently truncate the body — data corruption that looks like success. Hence the
    /// `completed` flag, which only [`HttpBodyStreamSender::finish`] sets: a channel that closed
    /// without it is an abort, and it is reported as [`HttpParseError::BodyStream`].
    pub async fn get_next_chunk(&self) -> Result<Option<Vec<u8>>, HttpParseError> {
        let mut rx = self.rx.lock().await;

        match rx.recv().await {
            Some(Ok(chunk)) => Ok(Some(chunk)),
            Some(Err(err)) => Err(err),
            None => {
                if self.completed.load(Ordering::Acquire) {
                    Ok(None)
                } else {
                    Err(HttpParseError::BodyStream(
                        "Request body stream ended unexpectedly".to_string(),
                    ))
                }
            }
        }
    }

    /// Polls the next chunk, for a transport that is itself a `Future` / `Body` — fl-url's
    /// `hyper::body::Body::poll_frame` on native, where the caller has a `Context` and no place to
    /// `.await`.
    ///
    /// Takes `&mut self`, so the `Mutex` is reached through `get_mut()` and never actually locked
    /// — no boxed future, no lock contention. That is the trade against
    /// [`get_next_chunk`](Self::get_next_chunk): exclusive ownership instead of `Arc`-sharing.
    ///
    /// Semantics are identical to [`get_next_chunk`](Self::get_next_chunk):
    /// `Poll::Ready(None)` means the body arrived **in full** (the `completed` flag is set), and a
    /// channel closed without [`HttpBodyStreamSender::finish`] is an abort reported as
    /// [`HttpParseError::BodyStream`] — never a silent EOF.
    pub fn poll_next_chunk(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<Vec<u8>, HttpParseError>>> {
        // `get_mut` — a `&mut self` proves nobody else holds the mutex, so there is nothing to lock.
        match self.rx.get_mut().poll_recv(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Some(item)) => std::task::Poll::Ready(Some(item)),
            std::task::Poll::Ready(None) => {
                if self.completed.load(Ordering::Acquire) {
                    std::task::Poll::Ready(None)
                } else {
                    std::task::Poll::Ready(Some(Err(HttpParseError::BodyStream(
                        "Request body stream ended unexpectedly".to_string(),
                    ))))
                }
            }
        }
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
