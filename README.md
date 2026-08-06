# my-http-utils

Shared, **wasm-compatible** foundation for describing HTTP models and building requests, plus the
low-level URL helpers it is built on.

Describe an HTTP model **once** with the derive macros: on the client the same model builds an
outgoing request; on the server ([my-http-server](https://github.com/MyJetTools/my-http-server),
which depends on my-http-utils) its own macros generate controllers and request parsing from the same
markup. my-http-utils has no hyper/tokio/server dependencies, so models compile to
`wasm32-unknown-unknown`.

## Workspace

- `my-http-utils` — the library.
- `http-request-schema-macros` — proc-macro crate, **re-exported as `my_http_utils::macros`** (use it
  from there, never directly).
- `tests` — integration tests for the macros.

## Features

- `default` — the **client request builder** only (`THttpRequestBuilder` + enum / custom-field
  `as_str`). Everything a browser client needs to build a request, and nothing else — no schema,
  no parsing — so wasm bundles stay small.
- `server` — additionally: the **OpenAPI/Swagger schema** (`get_input_params` / `get_model_routes`
  / `DataTypeProvider`, the `schema::{data_types, in_parameters, out_results}` modules,
  `MyHttpObjectStructure` / `MyHttpInputObjectStructure`), the
  [`http_input`](#server-side-parsing-server-feature) **parse engine**, and the derive-generated
  **`parse`** / `READS_BODY` / `STREAMS_BODY`. All of it is a server concern; all of it is still
  wasm-safe, just not compiled into clients that don't ask for it. The one dependency it adds is
  `tokio` with **only** the `sync` feature — the mpsc channel behind
  [`HttpBodyAsStream`](#streaming-the-request-body-server-feature). No runtime, no mio, no
  transport, and still no hyper.

  Note the `http_input` **field types** (`RawData`, `RawDataTyped<T>`, `FileContent`,
  `HttpBodyAsStream`, `HttpParseError`, `PasswordHttpInputField`) are **not** gated — a model
  shared between a client
  and a server names them, so they must compile for a wasm client too. Only the engine is gated.

## Main types

### Describing a model (`my_http_utils::macros`)

| macro | what it's for |
|---|---|
| `MyHttpInput` | derive on a request model: emits the client request builder — plus, under the `server` feature, the schema and the sync `parse` |
| `MyHttpInputObjectStructure` | a nested object **read from a request** (a `#[http_body]` field). Emits both halves of the wire contract — the client writer and the reader — plus the schema. Needs no serde |
| `MyHttpObjectStructure` | a nested object **written to a response**. The writer and the schema, without the read half |
| `MyHttpStringEnum` / `MyHttpIntegerEnum` | use an enum as a parameter value |
| `#[http_input_field]` | define a custom `String`-wrapper field type (the built-in `PasswordHttpInputField` is one) |
| `#[json_name("…")]` | name a nested object's field on the wire (see [Nested objects](#nested-objects-naming-keys-json_name)) |

### Field markup

Every field of a `MyHttpInput` model is tagged with exactly one of these, saying where the value
goes in the request:

| attribute | where the field goes |
|---|---|
| `#[http_path(name = "…")]` | a URL **path** segment (substituted into the route) |
| `#[http_query(name = "…")]` | a **query-string** parameter |
| `#[http_header(name = "…")]` | a request **header** |
| `#[http_body(name = "…")]` | one **root key of the JSON body** object |
| `#[http_form_data(name = "…")]` | one **`multipart/form-data`** field |
| `#[http_body_raw]` | the **entire body IS this one field** — verbatim `Vec<u8>` (or `RawData` / `RawDataTyped<T>` / `String`) |
| `#[http_body_as_stream]` | the **entire body is read as a stream**, chunk by chunk — never materialised (`HttpBodyAsStream`) |

Common params on every field attribute: `name`, `description`, `default`, `validator`, `trim`,
`to_lowercase`, `to_uppercase`, `print_request_to_console`. On the client these shape the outgoing
value (trim → case → validator); `default` only marks the schema param non-required.

**Body kinds are mutually exclusive.** There are **four**, and a model may use **at most one**:

- `#[http_body]` — the JSON body is an object of the named body fields (`{"name": …, "age": …}`).
- `#[http_form_data]` — the body is `multipart/form-data`, one part per field.
- `#[http_body_raw]` — the whole body is a single field: verbatim `Vec<u8>`, `RawData`, `RawDataTyped<T>`, or `String`.
- `#[http_body_as_stream]` — the whole body is read as a stream of chunks and is never held in
  memory whole (see [Streaming the request body](#streaming-the-request-body-server-feature)).

`#[http_path]` / `#[http_query]` / `#[http_header]` combine freely with any one body kind. Mixing
two body kinds in one model is a **compile error** ("choose one of …") — the last one especially:
a body cannot be materialised and streamed at the same time.

`#[http_body_as_stream]` takes only `name` and `description`. The other field directives (`trim`,
`to_lowercase`, `to_uppercase`, `validator`, `default`) have nothing to act on — the value never
exists as a string — and the field can not be `Option` (a compile error).

#### Three ways to describe a JSON body

There is no separate "whole body" attribute for a JSON object — **the body is assembled from the
fields you mark**. Which of these you want is usually decided by whether the payload has a name of
its own:

**1. Several `#[http_body]` fields — the fields together *are* the body.** Each one is a root key of
the JSON object; no wrapper struct is involved. This is the common case:

```rust
#[derive(MyHttpInput)]
pub struct CreateUser {
    #[http_body(name = "name", description = "User name")]
    pub name: String,
    #[http_body(name = "age", description = "User age")]
    pub age: u32,
}
// body: {"name":"John","age":42}
```

**2. A `#[http_body]` field typed as a nested object** — for a payload that has its own name and is
worth describing (and showing in Swagger) as a model. The nested type derives
`MyHttpInputObjectStructure`, and it composes freely with the flat fields above:

```rust
#[derive(MyHttpInputObjectStructure)]      // no serde needed
pub struct BankCard {
    pub card_number: String,
    pub exp_month: String,
}

#[derive(MyHttpInput)]
pub struct Pay {
    #[http_body(name = "challengeId", description = "Challenge id")]
    pub challenge_id: String,
    #[http_body(name = "card", description = "Card to charge")]
    pub card: Option<BankCard>,      // Option -> the key is omitted when None
}
// body: {"challengeId":"c1","card":{"card_number":"4111…","exp_month":"12"}}
```

**3. One `#[http_body_raw]` field — the body *is* that field**, verbatim, with no JSON object built
around it (see [`RawDataTyped<T>`](#read-a-typed-raw-body-with-deserialize_json-server-feature)).

Kinds 1 and 2 are the same kind and mix freely — both are `#[http_body]`. Kind 3 is exclusive with
them: a model has either named body fields or one raw body, never both.

### Nested objects: naming keys (`#[json_name]`)

A nested `#[http_body]` object is written **and** read by the derive itself — `my-json` on both
sides, no serde anywhere in this path. Both halves are generated from the same fields and the same
keys, so they cannot drift apart.

Name a key with **`#[json_name("…")]`**:

```rust
// No serde derive, and no `serde` dependency needed at all.
#[derive(MyHttpInputObjectStructure)]
pub struct BankCard {
    #[json_name("cardNumber")]
    pub card_number: String,
    pub exp_month: String,     // no attribute -> the Rust field name
}
```

`#[serde(rename = "…")]` and `#[serde(rename_all = "…")]` are honoured too, so a model that already
carries them — or that also travels through serde inside a `RawDataTyped<T>` — needs no second
spelling. All eight `rename_all` rules match serde byte for byte (`rename_all_matches_serde` in
`tests/src/parse_tests.rs` asserts it against `serde_json`'s own output, including the two
counter-intuitive ones: for *fields*, `lowercase` and `snake_case` are no-ops).

Precedence: `#[json_name]` > `#[serde(rename)]` > `#[serde(rename_all)]` > the Rust field name. The
OpenAPI schema documents whichever name wins, so Swagger always shows what actually goes on the
wire.

These are **compile errors** rather than silent mismatches:

- `#[json_name]` and `#[serde(rename)]` on one field naming it *differently* — serde still reads the
  model if it travels inside a `RawDataTyped<T>`, so two names would mean two wire formats.
- `#[serde(rename_all(serialize = …, deserialize = …))]` and `#[serde(rename(serialize = …,
  deserialize = …))]` — they give the two directions different names, and there is only one key.

> **serde attributes that change the object's *shape* are not honoured** — `skip` /
> `skip_serializing` / `skip_deserializing` / `skip_serializing_if`, `flatten`, `transparent`,
> `with` / `serialize_with` / `deserialize_with`. This path does not call serde, so they have no
> effect on it. If you need the full serde semantics, carry the payload as
> `#[http_body_raw] RawDataTyped<T>`, which is serde on both sides.

A nested object can only be read out of a **JSON body**: the reader borrows the request bytes, and
every other source would have to hand it a percent-decoded temporary. A struct-typed
`#[http_query]` / `#[http_header]` / `#[http_form_data]` field is an enum or an `#[http_input_field]`
type — both carry their own conversion and are unaffected.

### Building a request (`my_http_utils::schema::client`)

| type | what it's for |
|---|---|
| `THttpRequestBuilder` | generated by `MyHttpInput`: `fill_url` / `fill_headers` / `get_body` turn a model into request parts |
| `HeaderBuilder` | sink a transport (e.g. fl-url) implements to receive headers |
| `HttpRequestBuildError` | returned when a field `validator` rejects the outgoing value |

### Bodies, URL, readers

| type | what it's for |
|---|---|
| `my_http_utils::UrlBuilder` | build / inspect a URL (path segments + query, TCP or unix-socket) |
| `my_http_utils::body::HttpRequestBody` | an outgoing body: `Json` / `UrlEncoded` / `FormData` / `Raw` / `Empty` |
| `my_http_utils::body::{FormDataBody, UrlEncodedBody}` | build `multipart/form-data` / `x-www-form-urlencoded` bodies |
| `my_http_utils::url_encoded_data_reader::UrlEncodedDataReader` | read `x-www-form-urlencoded` (query strings / bodies) |
| `my_http_utils::form_data_reader::FormDataReader` | read `multipart/form-data` |
| `my_http_utils::url_encoder` / `my_http_utils::url_decoder` | percent encode / decode |

### Schema (`my_http_utils::schema`)

`data_types` (`DataTypeProvider`, `HttpDataType`, …), `in_parameters` (`HttpInputParameter`, …) and
`out_results` describe a model's shape — used for OpenAPI/Swagger on the server and for parameter
metadata. Generated by the derives; you rarely touch these directly. **These modules, and the
`get_input_params` / `get_model_routes` / `DataTypeProvider` code the derives emit, are behind the
`server` feature** — browser clients don't build OpenAPI, so they don't carry any of it.

### Server-side parsing (`server` feature)

With the `server` feature on, the same `MyHttpInput` markup **also** parses an incoming request —
so `my-http-server` no longer needs its own parsing derive. It stays transport-free: the whole
`http_input` layer is wasm-safe and knows nothing about hyper/tokio.

The module is split so the **types live at the root of `http_input`** and the **parse engine lives
in `http_input::core`**:

- **`http_input`** (root) — the types a model or a custom field touches: `HttpParseError`, the
  body/file field types `RawData` / `RawDataTyped<T>` / `FileContent`, the ready-made custom field
  `PasswordHttpInputField`, and (behind `server`) the value type `HttpInputValue`.
- **`http_input::core`** — the engine: the `THttpRequest` trait, the query / body readers, the
  `&str → T` converters, the source tags (`core::data_src::SRC_*`), and the `HttpInputValue → field`
  conversions.

**The gate follows that split, not the module.** The field types are compiled unconditionally —
a `#[http_body_raw] body: RawDataTyped<T>` or `#[http_body_as_stream] body: HttpBodyAsStream` model
must compile in an `*-api-shared` crate that the wasm client builds **without** `server`, which is
the whole point of the shared-model pattern. Only the engine (`HttpInputValue`, `http_input::core`
minus the `data_src` tags, and the channel machinery inside `HttpBodyAsStream`) is behind `server`,
so a client still carries none of the parsing code — and no tokio.

`MyHttpInput` then additionally generates, for `Model`:

```rust
impl Model {
    /// `true` when the model reads the body (`http_body` / `http_body_raw` / `http_form_data`),
    /// so the server can skip reading the body when it isn't needed.
    pub const READS_BODY: bool;

    /// `true` when the model takes the body as a stream (`http_body_as_stream`). Mutually
    /// exclusive with READS_BODY — a body is either materialised or streamed, never both.
    /// Emitted for every model, so a server that reads it needs no per-model knowledge.
    pub const STREAMS_BODY: bool;

    /// Synchronous — the server reads the body first (if READS_BODY) and exposes it via the trait.
    pub fn parse(request: &impl my_http_utils::http_input::core::THttpRequest)
        -> Result<Self, my_http_utils::http_input::HttpParseError>;
}
```

The signature is **the same for every model**, so a caller that only knows the type name can call
`parse`. Everything a request exposes is abstracted behind **one trait**, `http_input::core::THttpRequest`,
that the server implements (tests implement it over in-memory data):

```rust
pub trait THttpRequest {
    fn get_query_string(&self) -> &str;              // parsed here (param[], flags, %-decode)
    fn get_header(&self, name: &str) -> Option<&str>; // case-insensitive; not %-decoded
    fn get_path_value(&self, name: &str) -> Option<&str>; // route already matched by the impl
    fn get_body(&self) -> &[u8];                     // body already received
    fn get_content_type(&self) -> Option<&str> { self.get_header("content-type") } // default

    // For STREAMS_BODY models. The channel is already created and being filled BEFORE `parse`
    // runs, so `parse` only moves the ready value into the field. Default `None`, so no existing
    // implementation needs a single change; the second call must return `None`.
    fn take_body_stream(&self) -> Option<HttpBodyAsStream> { None } // default
}
```

my-http-utils does all the work on top of those primitives: query-string decoding, header/path
lookup, body content-type dispatch (`json` / `x-www-form-urlencoded` / `multipart/form-data`), and
the value → field-type conversions. The concrete value is [`HttpInputValue`] (the port of the
server's `EncodedParamValue`) with a full `TryInto<T>` set — `String`, `bool`, all ints/floats,
`DateTimeAsMicroseconds`, `Vec<T>` / `HashMap<String, V>` (from JSON), `RawData`, `RawDataTyped<T>`,
`FileContent`. Enums (`MyHttpStringEnum` / `MyHttpIntegerEnum`) and `#[http_input_field]` types also
get a `TryFrom<HttpInputValue>` so they parse too.

| type | what it's for |
|---|---|
| `http_input::core::THttpRequest` | the one trait the server (or a test) implements |
| `http_input::HttpInputValue` | a single read value, before conversion to a field's type |
| `http_input::HttpParseError` | parse failure: `RequiredParameterIsMissing{name,src}`, `CanNotParseValue{name,src,value}`, `UrlDecodeError`, `InvalidBodyFormat`, `NotSupportedContentType`, `Forbidden`, `Validation`, `BodyStream` |
| `http_input::{RawData, RawDataTyped<T>, FileContent}` | body/file field types: verbatim bytes / verbatim bytes the handler turns into `T` on demand via `RawDataTyped::deserialize_json` / an uploaded `multipart/form-data` file |
| `http_input::{HttpBodyAsStream, HttpBodyReader, HttpBodyStreamSender}` | the `#[http_body_as_stream]` field type, the handler's chunk reader, and the half the transport fills |
| `http_input::PasswordHttpInputField` | a ready-made `#[http_input_field]` type — a `String` rendered as OpenAPI `password` |
| `http_input::core::data_src::SRC_*` | source tags carried by values/errors (`Path`, `QueryString`, `Header`, `BodyJson`, …) |

`HttpParseError` keeps enough data (`name` / `src` / `value`) for the server to rebuild the exact
same `HttpFailResult` (status + text) it used to produce inline, via its own
`From<HttpParseError> for HttpFailResult`.

JSON body members are read from their **verbatim source text** (via `my-json`'s zero-copy
`JsonValueRef`), so a
number keeps its exact scale/precision (`100.00` stays `100.00`; a 128-bit integer isn't rounded
through `f64`) and a `RawData` / `RawDataTyped` field gets the member's original bytes untouched.

**Per-source semantics** (1:1 with the old server codegen):

- `#[http_path]` — required (Option is a compile error); the segment is percent-decoded.
- `#[http_query]` — Option / required / `default`; `Vec<T>` reads every repeat of the name.
- `#[http_header]` — Option / required / `default`; case-insensitive, taken verbatim.
- `#[http_body]` / `#[http_form_data]` — named body fields; the impl dispatches JSON vs form-data.
- `#[http_body_raw]` — non-Option takes the **whole body verbatim as `Vec<u8>`** and builds the field
  from those bytes via the crate-local `FromRawBody`: `Vec<u8>` as-is, `RawData` / `RawDataTyped<T>`
  keep the bytes untouched, `String` via a utf-8 check. No content-type parsing, so a binary /
  non-object body is never rejected up front. **`RawDataTyped<T>` defers JSON parsing:** `parse` only
  stores the raw body, so the server handler must call `body.deserialize_json()` to get the typed `T`
  — that is the single place `T` is produced and where a JSON error (if any) surfaces, never during
  `parse`. An **Option** `#[http_body_raw]` reads a *named* body field instead.
- `#[http_body_as_stream]` — never `Option` (a compile error). `parse` reads nothing: it moves the
  already-live `HttpBodyAsStream` out of `THttpRequest::take_body_stream()` into the field, and
  fails with `HttpParseError::BodyStream` if the implementation has none to give.
- `trim` / `to_lowercase` / `to_uppercase` apply to `String` fields after reading.

**Validators.** `validator = "fn"` uses the **same** contract as the client builder —
`fn(&str) -> Result<(), impl ToString>` (no `ctx`; put context-dependent checks in the action). On
parse it runs on the field's string form and its error becomes `HttpParseError::Validation`. Using
one contract lets a single validator function serve both the client build and the server parse.

## Examples

### Describe a model

```rust
use my_http_utils::macros::*;
use serde::Serialize;

#[derive(Serialize, MyHttpInput)]
pub struct CreateUser {
    #[http_path(name = "orgId", description = "Organisation id")]
    pub org_id: String,
    #[http_query(name = "notify", description = "Send a welcome email")]
    pub notify: bool,
    #[http_header(name = "X-Api-Key", description = "API key")]
    pub api_key: String,
    #[http_body(name = "name", description = "User name", trim)]
    pub name: String,
}
```

### Build a request from it

```rust
use my_http_utils::UrlBuilder;
use my_http_utils::schema::client::THttpRequestBuilder;

let mut url = UrlBuilder::new("https://api.example.com");
model.fill_url(&mut url)?;                 // "https://api.example.com/42?notify=true"
model.fill_headers(&mut headers)?;         // your transport's HeaderBuilder
let body = model.get_body()?;              // HttpRequestBody::Json(...)
```

A thin adapter (fl-url on native, fetch/gloo on wasm) turns `UrlBuilder` + headers +
`HttpRequestBody` into a real request.

### Read a query string

```rust
use my_http_utils::url_encoded_data_reader::UrlEncodedDataReader;

let reader = UrlEncodedDataReader::new("name=hello+world&id=5")?;
let name = reader.get_required("name")?.as_string()?; // "hello world"
```

### Parse a model from a request (`server` feature)

Implement `THttpRequest` once (the server does this over hyper; here it's a plain in-memory struct),
then any model parses through it:

```rust
use my_http_utils::http_input::core::THttpRequest;
use my_http_utils::macros::*;

struct InMemory { query: String, body: Vec<u8>, ctype: Option<String> }

impl THttpRequest for InMemory {
    fn get_query_string(&self) -> &str { &self.query }
    fn get_header(&self, _n: &str) -> Option<&str> { None }
    fn get_path_value(&self, _n: &str) -> Option<&str> { None }
    fn get_body(&self) -> &[u8] { &self.body }
    fn get_content_type(&self) -> Option<&str> { self.ctype.as_deref() }
}

#[derive(MyHttpInput)]
struct AddUser {
    #[http_query(name = "notify", description = "", default = false)]
    notify: bool,
    #[http_body(name = "name", description = "", trim)]
    name: String,
}

let request = InMemory {
    query: "notify=true".into(),
    body: br#"{"name":"  John "}"#.to_vec(),
    ctype: Some("application/json".into()),
};

let model = AddUser::parse(&request)?;      // notify = true, name = "John"
assert!(AddUser::READS_BODY);               // it has an http_body field
```

### Read a typed raw body with `deserialize_json` (`server` feature)

A `#[http_body_raw]` field typed as `RawDataTyped<T>` captures the **whole body verbatim**; `parse`
does *not* parse the JSON. The server handler turns it into `T` on demand via `deserialize_json()` —
the single place `T` is produced, and where a JSON error (if any) surfaces:

```rust
use my_http_utils::http_input::RawDataTyped;
use my_http_utils::macros::*;
use serde::Deserialize;

#[derive(Deserialize, MyHttpObjectStructure)]        // server side needs only Deserialize
struct AuditFilter { account_id: String, limit: i32 }

#[derive(MyHttpInput)]
struct QueryAudit {
    // the whole body IS this one field; OpenAPI shows `AuditFilter`'s structure
    #[http_body_raw(description = "Audit query filter")]
    body: RawDataTyped<AuditFilter>,
}

let model = QueryAudit::parse(&request)?;                  // body captured verbatim, not yet parsed
let filter: AuditFilter = model.body.deserialize_json()?; // <- deserialize in the handler
```

To *build* the same model as a client request, add `Serialize` to the payload and use `into()`
(`RawDataTyped<T>: From<T>` serialises it into the body): `QueryAudit { body: filter.into() }`.

This is the **shared-model** shape: the model above compiles as-is in an `*-api-shared` crate that
the server builds with `server` and the wasm client builds without it — `RawDataTyped<T>` and the
other `http_input` field types are not feature-gated (only the parse engine is), and
`deserialize_json` is available in both. It is also the one body shape that is **serde on both
sides** — the client serialises `T` with serde and the server deserialises it with serde — so unlike
a nested `#[http_body]` object it honours every serde attribute.

### Streaming the request body (`server` feature)

Every other body kind materialises the body: `parse` is synchronous and reads it out of
`THttpRequest::get_body() -> &[u8]`. For large uploads and proxy scenarios that is exactly wrong —
`#[http_body_as_stream]` hands the handler a **stream of chunks** instead, and the body is never
held in memory whole:

```rust
use my_http_utils::http_input::HttpBodyAsStream;
use my_http_utils::macros::*;

#[derive(MyHttpInput)]
pub struct UploadHttpInput {
    #[http_header(name = "X-File-Name", description = "File name")]
    pub file_name: String,

    #[http_body_as_stream(description = "File content")]
    pub body: HttpBodyAsStream,
}

// in the handler:
let reader = input_data.body.get_body_reader()?;
let expected = reader.get_content_length();      // Some(n) with Content-Length, None when chunked

while let Some(chunk) = reader.get_next_chunk().await? {
    // chunk: Vec<u8>
}
```

`get_next_chunk` takes **`&self`** (the receiver sits behind a `tokio::sync::Mutex`), so the reader
can be put into an `Arc` and read from several places. `reader.read_to_end(max_size)` collects the
rest into memory, with `max_size` as the safety valve.

**Split of responsibilities.** my-http-utils owns the channel and the *reading* half only —
`tokio/sync` and nothing else, no hyper, no `tokio::spawn`, no idea where the bytes come from.
my-http-server owns the complexity: it takes `hyper::body::Incoming`, loops over the frames, and
fills the sending half:

| type | who holds it |
|---|---|
| `HttpBodyAsStream` | the model field. `create(buffer, content_length)` makes the pair; `empty()` is the client-side "nothing to stream" |
| `HttpBodyReader` | the handler, via `get_body_reader()` — **once**; there is exactly one receiver, and a second call is an `Err` |
| `HttpBodyStreamSender` | the transport: `send_chunk` / `send_error` / `closed` / `finish` |

**Back pressure is the point.** The channel is *bounded* (`BODY_STREAM_DEFAULT_BUFFER` = 4 by
default), so memory per request is capped at roughly `buffer × chunk_size`. A pump that runs into a
full channel parks on `send().await`, and that pressure propagates down to the TCP window. An
unbounded channel would let a fast uploader eat memory instead.

**A truncated body is an error, not an EOF.** A closed channel means "all senders dropped" — which
happens both at a clean end *and* when the pump dies half-way. Treating the second as EOF would
silently truncate the body. So the transport calls `finish()` right before dropping the sender, and
a channel that ended without it yields `HttpParseError::BodyStream` rather than `Ok(None)`.

**Such a model is not a client request.** It describes an *incoming* body, so its generated
`get_body()` returns an `HttpRequestBuildError` instead of an empty body. The model still compiles
for wasm without `server` (`HttpBodyAsStream` itself is not gated) — it only fails at runtime, and
only if something actually tries to build an outgoing request out of it.

## wasm

my-http-utils is wasm-compatible (`wasm32-unknown-unknown`): no hyper, no server-only code —
including the `server` parse layer, which is wasm-safe but simply left out of clients that don't
enable the feature. The default build pulls **no tokio at all**; `server` adds `tokio/sync` only
(the `HttpBodyAsStream` channel), which is platform-independent and compiles for
`wasm32-unknown-unknown` too — verified with `cargo build --target wasm32-unknown-unknown` both with
and without the feature.

## Tests

```sh
cargo test --workspace --all-features
```

`--workspace` matters: the integration `tests` crate is a workspace member (not a dependency of the
root library), so a bare `cargo test` only runs the root library's own unit tests and **skips it**.
That crate always enables `server`, and exercises the client request builder, the derive-generated
`parse` end to end, and the body-stream channel (`tests/src/parse_tests.rs`,
`tests/src/body_stream_tests.rs`, `tests/src/lib.rs`).
