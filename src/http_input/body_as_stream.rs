//! Reading the request body **as a stream of chunks**, instead of materialising it whole.
//!
//! The split of responsibilities is deliberate and hard:
//!
//! * **my-http-utils (this module)** owns the channel and the *reading* half. It knows nothing
//!   about where the bytes come from — no hyper, no `tokio::spawn`, no transport at all. The only
//!   dependency is `tokio::sync` (mpsc + `Mutex`), which is platform-independent.
//! * **my-http-server** owns the complexity: it takes `hyper::body::Incoming`, loops over the
//!   frames and pushes the chunks into the sending half ([`HttpBodyStreamSender`]).
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
//! // in the handler:
//! let reader = input_data.body.get_body_reader()?;
//! let expected = reader.get_content_length();   // Some(n) with Content-Length, None when chunked
//! while let Some(chunk) = reader.get_next_chunk().await? {
//!     // chunk: Vec<u8>
//! }
//! ```
//!
//! **Feature gating.** [`HttpBodyAsStream`] itself is declared *unconditionally* — a model living
//! in an `*-api-shared` crate that a wasm client builds **without** `server` names the type, so it
//! must compile there. The guts (the channel, the sender, the reader) are behind `server`, exactly
//! like `HttpInputValue` vs the ungated `RawDataTyped`.

#[cfg(feature = "server")]
use crate::http_input::HttpParseError;

#[cfg(feature = "server")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "server")]
use std::sync::Arc;

/// Default capacity of the chunk channel: how many chunks the pump may read ahead of the handler.
///
/// The channel is **bounded**, so this is also the memory ceiling per request:
/// roughly `BODY_STREAM_DEFAULT_BUFFER × chunk_size`.
pub const BODY_STREAM_DEFAULT_BUFFER: usize = 4;

/// Chunks of the request body. Owns the receiving half of the channel; the transport layer
/// (my-http-server, which knows about hyper) fills it. Nothing here knows about hyper.
///
/// Obtained by `parse` for a `#[http_body_as_stream]` field. Take the reader out with
/// [`get_body_reader`](Self::get_body_reader) — exactly once.
pub struct HttpBodyAsStream {
    #[cfg(feature = "server")]
    inner: std::sync::Mutex<BodyStreamState>,
    content_length: Option<u64>,
}

/// The receiving half plus the "did the body actually finish?" flag.
///
/// A three-state enum rather than a plain `Option`, because the two failure modes of
/// [`HttpBodyAsStream::get_body_reader`] must be told apart: *there never was a stream*
/// (a client-side [`HttpBodyAsStream::empty`]) vs *the reader has already been taken*.
#[cfg(feature = "server")]
enum BodyStreamState {
    /// No stream at all — a model built on the client / wasm side.
    NotAvailable,
    /// The receiving half, not taken yet.
    Ready(BodyStreamInner),
    /// [`HttpBodyAsStream::get_body_reader`] has already handed the reader out. There is exactly
    /// one receiver, so there is no second one to give.
    Taken,
}

#[cfg(feature = "server")]
struct BodyStreamInner {
    rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, HttpParseError>>,
    completed: Arc<AtomicBool>,
}

impl HttpBodyAsStream {
    /// Client side / wasm: there is nothing to stream. Every [`get_body_reader`](Self::get_body_reader)
    /// on it fails — a model built to *send* a request never has an incoming body.
    pub fn empty() -> Self {
        Self {
            #[cfg(feature = "server")]
            inner: std::sync::Mutex::new(BodyStreamState::NotAvailable),
            content_length: None,
        }
    }

    /// Creates the pair «filler + model field». Called by the transport layer.
    ///
    /// `buffer` is the capacity of the **bounded** channel — the back-pressure knob. A pump that
    /// runs into a full channel parks on `send().await`, and that pressure propagates all the way
    /// down to the TCP window; an unbounded channel would instead let a fast uploader eat memory.
    /// See [`BODY_STREAM_DEFAULT_BUFFER`]. `0` is treated as `1` (tokio's channel rejects `0`).
    ///
    /// `content_length` is the value of the `Content-Length` header when present, `None` for a
    /// chunked body.
    #[cfg(feature = "server")]
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
    /// Takes `&self` — `parse` puts the model together behind a shared reference, and the handler
    /// reads the body out of the already-built model. The `Option::take` happens under a
    /// **synchronous** `Mutex` whose guard never crosses an `.await` (it is dropped before this
    /// function returns anything), so the caller's future stays `Send`.
    #[cfg(feature = "server")]
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

/// The sending half. Taken by my-http-server, which pours the chunks it pulls out of hyper into it.
#[cfg(feature = "server")]
pub struct HttpBodyStreamSender {
    tx: tokio::sync::mpsc::Sender<Result<Vec<u8>, HttpParseError>>,
    completed: Arc<AtomicBool>,
}

#[cfg(feature = "server")]
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

/// The reading half, handed to the handler by [`HttpBodyAsStream::get_body_reader`].
///
/// Every method takes `&self` (the receiver sits behind a `tokio::sync::Mutex`), so the reader can
/// be put into an `Arc` and read from several places.
#[cfg(feature = "server")]
pub struct HttpBodyReader {
    rx: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Result<Vec<u8>, HttpParseError>>>,
    completed: Arc<AtomicBool>,
    content_length: Option<u64>,
}

#[cfg(feature = "server")]
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
