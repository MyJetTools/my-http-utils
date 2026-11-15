//! `#[http_form_data]` fields through `FormDataProperty`. The server half reads each field out of a
//! part's header and content; the client half writes it as a part. Both halves are tested, and so
//! is the round trip: the client's own body parsed back by the server half.

use my_http_utils::body::{FormDataBody, HttpRequestBody};
use my_http_utils::form_data_reader::FormDataHeader;
use my_http_utils::http_input::{
    FileContent, FormDataProperty, HttpParseError, PasswordHttpInputField, RawData,
};
use my_http_utils::macros::*;
use my_http_utils::schema::client::{RandomStringGenerator, THttpRequestBuilder};
use my_http_utils::schema::data_types::{
    ArrayElement, DataTypeProvider, HttpDataType, HttpSimpleType,
};
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::parse_tests::FakeRequest;

// ---- helpers ------------------------------------------------------------------------------------

struct FixedBoundary;

impl RandomStringGenerator for FixedBoundary {
    fn generate_random_string(_len: usize) -> String {
        "TESTBOUNDARY0001".to_string()
    }
}

/// The multipart body `model` sends, as `(Content-Type, bytes)`.
fn form_data_of(model: impl THttpRequestBuilder) -> (String, Vec<u8>) {
    match model.get_body::<FixedBoundary>().unwrap() {
        HttpRequestBody::FormData(fd) => (fd.get_content_type(), fd.into_bytes()),
        _ => panic!("expected a multipart body"),
    }
}

/// Sends `model` as the client does, and hands its body back as an incoming request.
fn round_trip(model: impl THttpRequestBuilder) -> FakeRequest {
    let (content_type, body) = form_data_of(model);
    FakeRequest::default().body(&content_type, body)
}

/// An incoming request with a multipart body built by hand.
fn multipart(fd: FormDataBody) -> FakeRequest {
    let content_type = fd.get_content_type();
    FakeRequest::default().body(&content_type, fd.into_bytes())
}

/// An incoming request whose multipart body is `parts`, written out verbatim — for the parts
/// `FormDataBody` would never write.
fn raw_multipart(parts: &[&[u8]]) -> FakeRequest {
    let boundary = "TESTBOUNDARY";
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(part);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    FakeRequest::default().body(&format!("multipart/form-data; boundary={}", boundary), body)
}

fn file(file_name: &str, content_type: &str, content: &[u8]) -> FileContent {
    FileContent {
        file_name: file_name.to_string(),
        content_type: content_type.to_string(),
        content: content.to_vec(),
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// The sample payload is stored with LF line ends; multipart needs CRLF.
fn lf_to_crlf(src: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(src.len() + src.len() / 16);
    for &c in src {
        if c == b'\n' {
            result.push(b'\r');
        }
        result.push(c);
    }
    result
}

// ---- FileContent --------------------------------------------------------------------------------

/// The shape of ca-api's `ImportCaInputModel`: one value and two files, one of them renamed.
#[derive(Debug, MyHttpInput)]
struct ImportCaInput {
    #[http_form_data(name = "caName", description = "CA Common Name")]
    ca_name: String,
    #[http_form_data(description = "Certificate")]
    cert: FileContent,
    #[http_form_data(name = "privateKey", description = "Private key")]
    private_key: FileContent,
}

#[derive(Debug, MyHttpInput)]
struct OptionalFileInput {
    #[http_form_data(name = "title", description = "")]
    title: String,
    #[http_form_data(name = "attachment", description = "")]
    attachment: Option<FileContent>,
}

#[derive(Debug, MyHttpInput)]
struct ManyFilesInput {
    #[http_form_data(name = "files", description = "")]
    files: Vec<FileContent>,
}

#[test]
fn file_content_parses_from_the_sample_multipart_payload() {
    #[derive(Debug, MyHttpInput)]
    struct UploadInput {
        #[http_form_data(description = "")]
        file_title: String,
        #[http_form_data(description = "")]
        file_content: FileContent,
    }

    let payload = include_bytes!("../../test_form_data_payload.txt");
    let request = FakeRequest::default().body(
        "multipart/form-data; boundary=------DataFormBoundary15c4050a1c8749f2a53abd41b27c9d0f",
        lf_to_crlf(payload),
    );

    let model = UploadInput::parse(&request).unwrap();

    let text = std::str::from_utf8(payload).unwrap();
    let svg_start = text.find("<?xml").unwrap();
    let svg_end = text.find("</svg>").unwrap() + "</svg>".len();

    assert_eq!(model.file_title, "test");
    assert_eq!(model.file_content.file_name, "apple.svg");
    assert_eq!(model.file_content.content_type, "image/svg+xml");
    assert_eq!(
        model.file_content.content,
        lf_to_crlf(&payload[svg_start..svg_end])
    );
}

#[test]
fn file_content_round_trips_byte_for_byte() {
    // What a multipart body has to carry untouched: NUL, CRLF at both ends and inside, bytes that
    // are not UTF-8, and a `--` line that is not a boundary.
    let cert = b"\r\n-----BEGIN CERTIFICATE-----\r\nMII\0B\x01\xff\xfe\r\n--\r\n-----END CERTIFICATE-----\r\n";
    let key = [0u8, b'\r', b'\n', 0, 0xff, b'\r', b'\n'];

    let sent = ImportCaInput {
        ca_name: "My CA".to_string(),
        cert: file("ca.pem", "application/x-pem-file", cert),
        private_key: file("key.der", "application/octet-stream", &key),
    };

    let parsed = ImportCaInput::parse(&round_trip(sent)).unwrap();

    assert_eq!(parsed.ca_name, "My CA");
    assert_eq!(parsed.cert, file("ca.pem", "application/x-pem-file", cert));
    assert_eq!(
        parsed.private_key,
        file("key.der", "application/octet-stream", &key)
    );
}

#[test]
fn file_content_is_sent_as_a_file_part() {
    let (_, body) = form_data_of(ImportCaInput {
        ca_name: "My CA".to_string(),
        cert: file("ca.pem", "application/x-pem-file", b"PEM"),
        private_key: file("key.pem", "", b"KEY"),
    });

    assert!(contains(
        &body,
        b"Content-Disposition: form-data; name=\"cert\"; filename=\"ca.pem\"\r\nContent-Type: application/x-pem-file\r\n\r\nPEM\r\n"
    ));
    // An empty content type is not written as an empty header.
    assert!(contains(
        &body,
        b"Content-Disposition: form-data; name=\"privateKey\"; filename=\"key.pem\"\r\nContent-Type: application/octet-stream\r\n\r\nKEY\r\n"
    ));
    assert!(contains(
        &body,
        b"Content-Disposition: form-data; name=\"caName\"\r\n\r\nMy CA\r\n"
    ));
}

#[test]
fn an_empty_file_round_trips() {
    let sent = ImportCaInput {
        ca_name: "x".to_string(),
        cert: file("empty.pem", "text/plain", b""),
        private_key: file("crlf.bin", "application/octet-stream", b"\r\n"),
    };

    let parsed = ImportCaInput::parse(&round_trip(sent)).unwrap();

    assert_eq!(parsed.cert, file("empty.pem", "text/plain", b""));
    assert_eq!(
        parsed.private_key,
        file("crlf.bin", "application/octet-stream", b"\r\n")
    );
}

#[test]
fn optional_file_content_is_sent_only_when_some() {
    let (content_type, body) = form_data_of(OptionalFileInput {
        title: "t".to_string(),
        attachment: None,
    });
    assert!(!contains(&body, b"name=\"attachment\""));
    let parsed =
        OptionalFileInput::parse(&FakeRequest::default().body(&content_type, body)).unwrap();
    assert_eq!(parsed.title, "t");
    assert_eq!(parsed.attachment, None);

    let (content_type, body) = form_data_of(OptionalFileInput {
        title: "t".to_string(),
        attachment: Some(file("a.bin", "application/octet-stream", b"\0\r\n\0")),
    });
    assert!(contains(&body, b"name=\"attachment\"; filename=\"a.bin\""));
    let parsed =
        OptionalFileInput::parse(&FakeRequest::default().body(&content_type, body)).unwrap();
    assert_eq!(
        parsed.attachment,
        Some(file("a.bin", "application/octet-stream", b"\0\r\n\0"))
    );
}

#[test]
fn vec_of_file_content_is_one_part_per_file_in_order() {
    let files = vec![
        file("1.bin", "application/octet-stream", b"\0one\r\n"),
        file("2.txt", "text/plain", b""),
        file("3.pem", "application/x-pem-file", b"three"),
    ];

    let parsed = ManyFilesInput::parse(&round_trip(ManyFilesInput {
        files: files.clone(),
    }))
    .unwrap();
    assert_eq!(parsed.files, files);

    // Nothing is sent for an empty Vec, and nothing reads back as an empty Vec.
    let (_, body) = form_data_of(ManyFilesInput { files: vec![] });
    assert!(!contains(&body, b"name=\"files\""));
    let parsed = ManyFilesInput::parse(&round_trip(ManyFilesInput { files: vec![] })).unwrap();
    assert!(parsed.files.is_empty());
}

#[test]
fn a_file_part_without_a_content_type_reads_as_text_plain() {
    let request = raw_multipart(&[
        b"Content-Disposition: form-data; name=\"files\"; filename=\"notes.txt\"\r\n\r\nhello",
    ]);

    let model = ManyFilesInput::parse(&request).unwrap();

    assert_eq!(model.files, vec![file("notes.txt", "text/plain", b"hello")]);
}

#[test]
fn a_value_part_where_a_file_is_expected_is_an_error() {
    let request = raw_multipart(&[
        b"Content-Disposition: form-data; name=\"caName\"\r\n\r\nMy CA",
        b"Content-Disposition: form-data; name=\"cert\"\r\n\r\nnot a file",
        b"Content-Disposition: form-data; name=\"privateKey\"; filename=\"k\"\r\nContent-Type: text/plain\r\n\r\nk",
    ]);

    let err = ImportCaInput::parse(&request).unwrap_err();
    assert_eq!(
        err,
        HttpParseError::NotSupportedContentType(
            "Field 'cert' is a form-data value, not a file".to_string()
        )
    );
}

#[test]
fn a_missing_file_is_a_missing_required_parameter() {
    let request = raw_multipart(&[b"Content-Disposition: form-data; name=\"caName\"\r\n\r\nMy CA"]);

    match ImportCaInput::parse(&request).unwrap_err() {
        HttpParseError::RequiredParameterIsMissing { name, src } => {
            assert_eq!(name, "cert");
            assert_eq!(src, "Body");
        }
        other => panic!("expected a missing cert, got {:?}", other),
    }
}

#[test]
fn a_file_can_not_come_from_an_url_encoded_body() {
    let request = FakeRequest::default().body(
        "application/x-www-form-urlencoded",
        "caName=x&cert=abc&privateKey=def",
    );

    assert_eq!(
        ImportCaInput::parse(&request).unwrap_err(),
        HttpParseError::NotSupportedContentType(
            "Field 'cert' is a form-data value, not a file".to_string()
        )
    );
}

#[test]
fn file_fields_are_binary_in_the_schema() {
    let params = ImportCaInput::get_input_params();
    let param = |name: &str| {
        params
            .iter()
            .find(|p| p.field.name == name)
            .unwrap_or_else(|| panic!("no input param {}", name))
    };

    for name in ["cert", "privateKey"] {
        let p = param(name);
        assert!(p.is_form_data());
        assert!(p.field.is_file_upload());
        assert!(matches!(
            p.field.data_type,
            HttpDataType::SimpleType(HttpSimpleType::Binary)
        ));
        assert!(p.field.required);
    }
    assert!(matches!(
        param("caName").field.data_type,
        HttpDataType::SimpleType(HttpSimpleType::String)
    ));

    let optional = OptionalFileInput::get_input_params();
    let attachment = optional
        .iter()
        .find(|p| p.field.name == "attachment")
        .unwrap();
    assert!(matches!(
        attachment.field.data_type,
        HttpDataType::SimpleType(HttpSimpleType::Binary)
    ));
    assert!(!attachment.field.required);

    let many = ManyFilesInput::get_input_params();
    assert!(matches!(
        many[0].field.data_type,
        HttpDataType::ArrayOf(ArrayElement::SimpleType(HttpSimpleType::Binary))
    ));
    assert!(!many[0].field.required);
}

// ---- the header and the content: each type takes what it needs ----------------------------------

#[derive(Debug, MyHttpInput)]
struct PemAsTextInput {
    #[http_form_data(name = "cert", description = "")]
    cert: String,
}

#[test]
fn a_string_field_takes_the_content_of_a_file_part() {
    // What `curl -F cert=@ca.pem` sends is a file part; a String field reads its content as text.
    const PEM: &[u8] = b"-----BEGIN CERTIFICATE-----\r\nMIIB\r\n-----END CERTIFICATE-----\r\n";
    let request = multipart(FormDataBody::new("B").append_form_data_file(
        "cert",
        "ca.pem",
        "application/x-pem-file",
        PEM,
    ));

    let model = PemAsTextInput::parse(&request).unwrap();

    assert_eq!(model.cert.as_bytes(), PEM);
}

#[test]
fn a_string_field_rejects_content_that_is_not_text() {
    let request = multipart(FormDataBody::new("B").append_form_data_file(
        "cert",
        "key.der",
        "application/octet-stream",
        &[0x30, 0x82, 0xff, 0xfe],
    ));

    assert_eq!(
        PemAsTextInput::parse(&request).unwrap_err(),
        HttpParseError::CanNotParseValue {
            name: "cert".to_string(),
            src: "FormData",
            value: "<4 bytes that are not UTF-8 text>".to_string(),
        }
    );
}

#[derive(MyHttpInput)]
struct RawPartInput {
    #[http_form_data(name = "blob", description = "")]
    blob: RawData,
}

#[test]
fn raw_data_takes_the_content_of_any_part() {
    let bytes = [0u8, 0xff, b'\r', b'\n', 1];

    // A file part.
    let model = RawPartInput::parse(&multipart(FormDataBody::new("B").append_form_data_file(
        "blob",
        "a.bin",
        "application/octet-stream",
        &bytes,
    )))
    .unwrap();
    assert_eq!(model.blob.as_slice(), bytes);

    // A value part that is not text. The older value/file view skips one; a part is a part.
    let mut part = b"Content-Disposition: form-data; name=\"blob\"\r\n\r\n".to_vec();
    part.extend_from_slice(&bytes);
    let model = RawPartInput::parse(&raw_multipart(&[&part])).unwrap();
    assert_eq!(model.blob.as_slice(), bytes);
}

#[test]
fn raw_data_is_sent_as_a_file_part_and_round_trips() {
    let bytes = vec![0u8, b'\r', b'\n', 0xff];

    let (_, body) = form_data_of(RawPartInput {
        blob: RawData::new(bytes.clone()),
    });
    assert!(contains(
        &body,
        b"name=\"blob\"; filename=\"blob\"\r\nContent-Type: application/octet-stream\r\n"
    ));

    let parsed = RawPartInput::parse(&round_trip(RawPartInput {
        blob: RawData::new(bytes.clone()),
    }))
    .unwrap();
    assert_eq!(parsed.blob.as_slice(), bytes.as_slice());
}

/// A type of the model's own: an uploaded PEM certificate, checked on the way in.
#[derive(Debug, PartialEq)]
struct PemCertificate(String);

impl FormDataProperty for PemCertificate {
    fn from_form_data_content(
        header: FormDataHeader<'_>,
        content: &[u8],
    ) -> Result<Self, HttpParseError> {
        let text = String::from_form_data_content(header, content)?;
        if !text.starts_with("-----BEGIN CERTIFICATE-----") {
            return Err(HttpParseError::Validation(format!(
                "'{}' is not a PEM certificate",
                header.name
            )));
        }
        Ok(Self(text))
    }

    fn write_form_data(&self, name: &'static str, form_data: FormDataBody) -> FormDataBody {
        form_data.append_form_data_file(
            name,
            "certificate.pem",
            "application/x-pem-file",
            self.0.as_bytes(),
        )
    }
}

impl DataTypeProvider for PemCertificate {
    fn get_data_type() -> HttpDataType {
        HttpDataType::SimpleType(HttpSimpleType::Binary)
    }
}

#[derive(Debug, MyHttpInput)]
struct CustomTypeInput {
    #[http_form_data(name = "cert", description = "")]
    cert: PemCertificate,
    #[http_form_data(name = "chain", description = "")]
    chain: Vec<PemCertificate>,
}

#[test]
fn a_type_of_the_models_own_reads_and_writes_itself() {
    const PEM: &str = "-----BEGIN CERTIFICATE-----\r\nMIIB\r\n-----END CERTIFICATE-----\r\n";
    let sent = || CustomTypeInput {
        cert: PemCertificate(PEM.to_string()),
        chain: vec![
            PemCertificate(PEM.to_string()),
            PemCertificate(PEM.to_string()),
        ],
    };

    let (_, body) = form_data_of(sent());
    assert!(contains(
        &body,
        b"name=\"cert\"; filename=\"certificate.pem\"\r\nContent-Type: application/x-pem-file\r\n"
    ));

    let parsed = CustomTypeInput::parse(&round_trip(sent())).unwrap();
    assert_eq!(parsed.cert, PemCertificate(PEM.to_string()));
    assert_eq!(
        parsed.chain,
        vec![
            PemCertificate(PEM.to_string()),
            PemCertificate(PEM.to_string())
        ]
    );

    // Its own check runs on the way in.
    let request = multipart(FormDataBody::new("B").append_form_data_file(
        "cert",
        "x.pem",
        "text/plain",
        b"not a certificate",
    ));
    assert_eq!(
        CustomTypeInput::parse(&request).unwrap_err(),
        HttpParseError::Validation("'cert' is not a PEM certificate".to_string())
    );
}

// ---- the other field types, and the other bodies a form-data model takes -------------------------

#[derive(Debug, Clone, Copy, PartialEq, MyHttpStringEnum)]
enum Algorithm {
    #[http_enum_case(id = "0", value = "rsa", description = "RSA", default)]
    Rsa,
    #[http_enum_case(id = "1", value = "ec", description = "EC")]
    Ec,
}

#[derive(MyHttpInput)]
struct ScalarsInput {
    #[http_form_data(name = "name", description = "")]
    name: String,
    #[http_form_data(name = "days", description = "")]
    days: u32,
    #[http_form_data(name = "ratio", description = "")]
    ratio: f64,
    #[http_form_data(name = "isCa", description = "")]
    is_ca: bool,
    #[http_form_data(name = "notAfter", description = "")]
    not_after: DateTimeAsMicroseconds,
    #[http_form_data(name = "algorithm", description = "")]
    algorithm: Algorithm,
    #[http_form_data(name = "password", description = "")]
    password: PasswordHttpInputField,
    #[http_form_data(name = "tags", description = "")]
    tags: Vec<String>,
    #[http_form_data(name = "comment", description = "")]
    comment: Option<String>,
}

fn not_after() -> DateTimeAsMicroseconds {
    DateTimeAsMicroseconds::from_str("2026-11-15T10:00:00Z").unwrap()
}

#[test]
fn every_field_type_round_trips() {
    let sent = ScalarsInput {
        name: "My \"CA\"".to_string(),
        days: 365,
        ratio: 0.5,
        is_ca: true,
        not_after: not_after(),
        algorithm: Algorithm::Ec,
        password: PasswordHttpInputField::new("p@ss"),
        tags: vec!["a".to_string(), "b c".to_string()],
        comment: None,
    };

    let (content_type, body) = form_data_of(sent);
    // A Vec is one part per item, and an enum goes out as its case value.
    assert!(contains(&body, b"name=\"tags\"\r\n\r\na\r\n"));
    assert!(contains(&body, b"name=\"tags\"\r\n\r\nb c\r\n"));
    assert!(contains(&body, b"name=\"algorithm\"\r\n\r\nec\r\n"));
    assert!(!contains(&body, b"name=\"comment\""));

    let parsed = ScalarsInput::parse(&FakeRequest::default().body(&content_type, body)).unwrap();

    assert_eq!(parsed.name, "My \"CA\"");
    assert_eq!(parsed.days, 365);
    assert_eq!(parsed.ratio, 0.5);
    assert!(parsed.is_ca);
    assert_eq!(
        parsed.not_after.unix_microseconds,
        not_after().unix_microseconds
    );
    assert_eq!(parsed.algorithm, Algorithm::Ec);
    assert_eq!(parsed.password.as_str(), "p@ss");
    assert_eq!(parsed.tags, vec!["a", "b c"]);
    assert_eq!(parsed.comment, None);
}

#[test]
fn a_form_data_model_still_reads_an_url_encoded_body() {
    let request = FakeRequest::default().body(
        "application/x-www-form-urlencoded",
        "name=My+CA&days=30&ratio=1.5&isCa=1&notAfter=2026-11-15T10%3A00%3A00Z\
         &algorithm=1&password=p%40ss&tags=a&tags[]=b",
    );

    let model = ScalarsInput::parse(&request).unwrap();

    assert_eq!(model.name, "My CA");
    assert_eq!(model.days, 30);
    assert_eq!(model.ratio, 1.5);
    assert!(model.is_ca);
    assert_eq!(
        model.not_after.unix_microseconds,
        not_after().unix_microseconds
    );
    // An enum is matched by its id as well.
    assert_eq!(model.algorithm, Algorithm::Ec);
    assert_eq!(model.password.as_str(), "p@ss");
    assert_eq!(model.tags, vec!["a", "b"]);
    assert_eq!(model.comment, None);
}

#[test]
fn a_form_data_model_still_reads_a_json_body() {
    let request = FakeRequest::default().body(
        "application/json",
        r#"{"name":"My \"CA\"","days":30,"ratio":1.50,"isCa":true,"notAfter":"2026-11-15T10:00:00Z",
            "algorithm":"rsa","password":"p","tags":["a","b\nc",7],"comment":null}"#,
    );

    let model = ScalarsInput::parse(&request).unwrap();

    assert_eq!(model.name, "My \"CA\"");
    assert_eq!(model.days, 30);
    assert_eq!(model.ratio, 1.5);
    assert!(model.is_ca);
    assert_eq!(model.algorithm, Algorithm::Rsa);
    // A string element unescaped, any other one verbatim.
    assert_eq!(model.tags, vec!["a", "b\nc", "7"]);
    // `null` is absent.
    assert_eq!(model.comment, None);

    // A member that is not an array is one item.
    let request = FakeRequest::default().body(
        "application/json",
        r#"{"name":"n","days":1,"ratio":1,"isCa":false,"notAfter":"2026-11-15T10:00:00Z",
            "algorithm":"ec","password":"p","tags":"only","comment":"c"}"#,
    );
    let model = ScalarsInput::parse(&request).unwrap();
    assert_eq!(model.tags, vec!["only"]);
    assert_eq!(model.comment.as_deref(), Some("c"));
}

#[test]
fn a_bad_scalar_names_its_field() {
    let request = raw_multipart(&[
        b"Content-Disposition: form-data; name=\"name\"\r\n\r\nn",
        b"Content-Disposition: form-data; name=\"days\"\r\n\r\nmany",
    ]);

    assert_eq!(
        ScalarsInput::parse(&request).err(),
        Some(HttpParseError::CanNotParseValue {
            name: "days".to_string(),
            src: "FormData",
            value: "many".to_string(),
        })
    );
}
