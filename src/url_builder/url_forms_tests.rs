//! Forms of an address, and what the builder makes of each.
//!
//! A row pins every getter at once, in the four states a caller can bring the builder
//! to, so a change in how a form is split shows up as a changed row.

use crate::{UrlBuilder, UrlBuilderInner, UrlBuilderUnixSocket};

fn caught(f: impl FnOnce() -> String) -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| "<panic>".to_string())
}

/// Every getter of the builder in one line; one that panics shows as `<panic>`.
fn snapshot(url: &UrlBuilder) -> String {
    let scheme = caught(|| {
        let scheme = url.get_scheme();
        if scheme.is_unix_socket() {
            "unix"
        } else if scheme.is_https() {
            "https"
        } else if scheme.is_ws() {
            "ws"
        } else if scheme.is_wss() {
            "wss"
        } else {
            "http"
        }
        .to_string()
    });

    format!(
        "{scheme} host={} host_port={} origin={} path={} query={} target={} endpoint={} url={}",
        caught(|| url.get_host().to_string()),
        caught(|| url.get_host_port().to_string()),
        caught(|| url.get_scheme_and_host().to_string()),
        caught(|| url.get_path().to_string()),
        caught(|| url.get_query().unwrap_or("-").to_string()),
        caught(|| url.get_path_and_query()),
        caught(|| {
            let endpoint = url.get_remote_endpoint(None);
            format!(
                "{},{}",
                endpoint.get_host(),
                endpoint.get_port_str().unwrap_or("-")
            )
        }),
        caught(|| url.to_string()),
    )
}

/// The builder as made from `src`, then after each way of adding to it:
/// 0. nothing added;
/// 1. `append_path_segment("seg")` + `append_query_param("k", Some("v"))`;
/// 2. `append_raw_ending("/raw?r=1")`;
/// 3. `append_query_param("k", Some("v"))` + `append_raw_ending("/raw?r=1")`.
fn states(src: &str) -> [String; 4] {
    let new = UrlBuilder::new(src);

    let mut seg_query = UrlBuilder::new(src);
    seg_query.append_path_segment("seg");
    seg_query.append_query_param("k", Some("v"));

    let mut raw = UrlBuilder::new(src);
    raw.append_raw_ending("/raw?r=1");

    let mut query_raw = UrlBuilder::new(src);
    query_raw.append_query_param("k", Some("v"));
    query_raw.append_raw_ending("/raw?r=1");

    [
        snapshot(&new),
        snapshot(&seg_query),
        snapshot(&raw),
        snapshot(&query_raw),
    ]
}

fn assert_states(src: &str, expected: [&str; 4]) {
    let states = states(src);

    for (no, (state, expected)) in states.iter().zip(expected).enumerate() {
        assert_eq!(state, expected, "{src:?}, state {no}");
    }
}

/// The first character after the scheme is a delimiter like any other, so an address
/// that names no host has an empty one, with the port, the path and the query each in
/// its own place. fl-url tells such an address by exactly that: its host is empty or
/// starts with a delimiter.
#[test]
fn an_address_with_no_host() {
    assert_states(
        "",
        [
            "http host= host_port= origin=http:// path=/ query=- target=/ endpoint=,- url=http://",
            "http host= host_port= origin=http:// path=/seg query=k=v target=/seg?k=v endpoint=,- url=http:///seg?k=v",
            "http host= host_port= origin=http:// path=/raw query=r=1 target=/raw?r=1 endpoint=,- url=http:///raw?r=1",
            "http host= host_port= origin=http:// path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=,- url=http://?k=v/raw?r=1",
        ],
    );

    assert_states(
        " ",
        [
            "http host=  host_port=  origin=http://  path=/ query=- target=/ endpoint= ,- url=http:// ",
            "http host=  host_port=  origin=http://  path=/seg query=k=v target=/seg?k=v endpoint= ,- url=http:// /seg?k=v",
            "http host=  host_port=  origin=http://  path=/raw query=r=1 target=/raw?r=1 endpoint= ,- url=http:// /raw?r=1",
            "http host=  host_port=  origin=http://  path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint= ,- url=http:// ?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://",
        [
            "http host= host_port= origin=http:// path=/ query=- target=/ endpoint=,- url=http://",
            "http host= host_port= origin=http:// path=/seg query=k=v target=/seg?k=v endpoint=,- url=http:///seg?k=v",
            "http host= host_port= origin=http:// path=/raw query=r=1 target=/raw?r=1 endpoint=,- url=http:///raw?r=1",
            "http host= host_port= origin=http:// path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=,- url=http://?k=v/raw?r=1",
        ],
    );

    assert_states(
        "https://",
        [
            "https host= host_port= origin=https:// path=/ query=- target=/ endpoint=,- url=https://",
            "https host= host_port= origin=https:// path=/seg query=k=v target=/seg?k=v endpoint=,- url=https:///seg?k=v",
            "https host= host_port= origin=https:// path=/raw query=r=1 target=/raw?r=1 endpoint=,- url=https:///raw?r=1",
            "https host= host_port= origin=https:// path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=,- url=https://?k=v/raw?r=1",
        ],
    );

    assert_states(
        "ws://",
        [
            "ws host= host_port= origin=ws:// path=/ query=- target=/ endpoint=,- url=ws://",
            "ws host= host_port= origin=ws:// path=/seg query=k=v target=/seg?k=v endpoint=,- url=ws:///seg?k=v",
            "ws host= host_port= origin=ws:// path=/raw query=r=1 target=/raw?r=1 endpoint=,- url=ws:///raw?r=1",
            "ws host= host_port= origin=ws:// path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=,- url=ws://?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://:8080",
        [
            "http host= host_port=:8080 origin=http://:8080 path=/ query=- target=/ endpoint=,8080 url=http://:8080",
            "http host= host_port=:8080 origin=http://:8080 path=/seg query=k=v target=/seg?k=v endpoint=,8080 url=http://:8080/seg?k=v",
            "http host= host_port=:8080 origin=http://:8080 path=/raw query=r=1 target=/raw?r=1 endpoint=,8080 url=http://:8080/raw?r=1",
            "http host= host_port=:8080 origin=http://:8080 path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=,8080 url=http://:8080?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http:///path",
        [
            "http host= host_port= origin=http:// path=/path query=- target=/path endpoint=,- url=http:///path",
            "http host= host_port= origin=http:// path=/path/seg query=k=v target=/path/seg?k=v endpoint=,- url=http:///path/seg?k=v",
            "http host= host_port= origin=http:// path=/path/raw query=r=1 target=/path/raw?r=1 endpoint=,- url=http:///path/raw?r=1",
            "http host= host_port= origin=http:// path=/path query=k=v/raw?r=1 target=/path?k=v/raw?r=1 endpoint=,- url=http:///path?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://?a=b",
        [
            "http host= host_port= origin=http:// path=/ query=a=b target=/?a=b endpoint=,- url=http://?a=b",
            "http host= host_port= origin=http:// path=/seg query=a=b&k=v target=/seg?a=b&k=v endpoint=,- url=http:///seg?a=b&k=v",
            "http host= host_port= origin=http:// path=/ query=a=b/raw?r=1 target=/?a=b/raw?r=1 endpoint=,- url=http://?a=b/raw?r=1",
            "http host= host_port= origin=http:// path=/ query=a=b&k=v/raw?r=1 target=/?a=b&k=v/raw?r=1 endpoint=,- url=http://?a=b&k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://#frag",
        [
            "http host=#frag host_port=#frag origin=http://#frag path=/ query=- target=/ endpoint=,- url=http://#frag",
            "http host=#frag host_port=#frag origin=http://#frag path=/seg query=k=v target=/seg?k=v endpoint=,- url=http://#frag/seg?k=v",
            "http host=#frag host_port=#frag origin=http://#frag path=/raw query=r=1 target=/raw?r=1 endpoint=,- url=http://#frag/raw?r=1",
            "http host=#frag host_port=#frag origin=http://#frag path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=,- url=http://#frag?k=v/raw?r=1",
        ],
    );
}

/// A scheme is looked for at the start of the address only. A `://` further on belongs
/// to the path or the query, and an address that has one there and no scheme of its own
/// is read like any other address with no scheme.
///
/// A scheme `Scheme` does not know reads as http, and still has an endpoint.
#[test]
fn a_scheme_is_only_what_the_address_starts_with() {
    assert_states(
        "host:8080/path?next=http://other",
        [
            "http host=host host_port=host:8080 origin=http://host:8080 path=/path query=next=http://other target=/path?next=http://other endpoint=host,8080 url=http://host:8080/path?next=http://other",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/path/seg query=next=http://other&k=v target=/path/seg?next=http://other&k=v endpoint=host,8080 url=http://host:8080/path/seg?next=http://other&k=v",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/path query=next=http://other/raw?r=1 target=/path?next=http://other/raw?r=1 endpoint=host,8080 url=http://host:8080/path?next=http://other/raw?r=1",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/path query=next=http://other&k=v/raw?r=1 target=/path?next=http://other&k=v/raw?r=1 endpoint=host,8080 url=http://host:8080/path?next=http://other&k=v/raw?r=1",
        ],
    );

    assert_states(
        "host/path?next=http://other",
        [
            "http host=host host_port=host origin=http://host path=/path query=next=http://other target=/path?next=http://other endpoint=host,- url=http://host/path?next=http://other",
            "http host=host host_port=host origin=http://host path=/path/seg query=next=http://other&k=v target=/path/seg?next=http://other&k=v endpoint=host,- url=http://host/path/seg?next=http://other&k=v",
            "http host=host host_port=host origin=http://host path=/path query=next=http://other/raw?r=1 target=/path?next=http://other/raw?r=1 endpoint=host,- url=http://host/path?next=http://other/raw?r=1",
            "http host=host host_port=host origin=http://host path=/path query=next=http://other&k=v/raw?r=1 target=/path?next=http://other&k=v/raw?r=1 endpoint=host,- url=http://host/path?next=http://other&k=v/raw?r=1",
        ],
    );

    assert_states(
        "host:abc/x://",
        [
            "http host=host host_port=host:abc origin=http://host:abc path=/x:// query=- target=/x:// endpoint=host,abc url=http://host:abc/x://",
            "http host=host host_port=host:abc origin=http://host:abc path=/x://seg query=k=v target=/x://seg?k=v endpoint=host,abc url=http://host:abc/x://seg?k=v",
            "http host=host host_port=host:abc origin=http://host:abc path=/x://raw query=r=1 target=/x://raw?r=1 endpoint=host,abc url=http://host:abc/x://raw?r=1",
            "http host=host host_port=host:abc origin=http://host:abc path=/x:// query=k=v/raw?r=1 target=/x://?k=v/raw?r=1 endpoint=host,abc url=http://host:abc/x://?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://host:8080/path?next=http://other",
        [
            "http host=host host_port=host:8080 origin=http://host:8080 path=/path query=next=http://other target=/path?next=http://other endpoint=host,8080 url=http://host:8080/path?next=http://other",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/path/seg query=next=http://other&k=v target=/path/seg?next=http://other&k=v endpoint=host,8080 url=http://host:8080/path/seg?next=http://other&k=v",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/path query=next=http://other/raw?r=1 target=/path?next=http://other/raw?r=1 endpoint=host,8080 url=http://host:8080/path?next=http://other/raw?r=1",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/path query=next=http://other&k=v/raw?r=1 target=/path?next=http://other&k=v/raw?r=1 endpoint=host,8080 url=http://host:8080/path?next=http://other&k=v/raw?r=1",
        ],
    );

    assert_states(
        "ftp://host:21/x",
        [
            "http host=host host_port=host:21 origin=ftp://host:21 path=/x query=- target=/x endpoint=host,21 url=ftp://host:21/x",
            "http host=host host_port=host:21 origin=ftp://host:21 path=/x/seg query=k=v target=/x/seg?k=v endpoint=host,21 url=ftp://host:21/x/seg?k=v",
            "http host=host host_port=host:21 origin=ftp://host:21 path=/x/raw query=r=1 target=/x/raw?r=1 endpoint=host,21 url=ftp://host:21/x/raw?r=1",
            "http host=host host_port=host:21 origin=ftp://host:21 path=/x query=k=v/raw?r=1 target=/x?k=v/raw?r=1 endpoint=host,21 url=ftp://host:21/x?k=v/raw?r=1",
        ],
    );
}

/// The colons inside the brackets belong to the address, and only one after the closing
/// bracket separates the port. The endpoint reads a literal the same way.
#[test]
fn an_ipv6_literal_is_a_host() {
    assert_states(
        "http://[::1]",
        [
            "http host=[::1] host_port=[::1] origin=http://[::1] path=/ query=- target=/ endpoint=[::1],- url=http://[::1]",
            "http host=[::1] host_port=[::1] origin=http://[::1] path=/seg query=k=v target=/seg?k=v endpoint=[::1],- url=http://[::1]/seg?k=v",
            "http host=[::1] host_port=[::1] origin=http://[::1] path=/raw query=r=1 target=/raw?r=1 endpoint=[::1],- url=http://[::1]/raw?r=1",
            "http host=[::1] host_port=[::1] origin=http://[::1] path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=[::1],- url=http://[::1]?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://[::1]/path",
        [
            "http host=[::1] host_port=[::1] origin=http://[::1] path=/path query=- target=/path endpoint=[::1],- url=http://[::1]/path",
            "http host=[::1] host_port=[::1] origin=http://[::1] path=/path/seg query=k=v target=/path/seg?k=v endpoint=[::1],- url=http://[::1]/path/seg?k=v",
            "http host=[::1] host_port=[::1] origin=http://[::1] path=/path/raw query=r=1 target=/path/raw?r=1 endpoint=[::1],- url=http://[::1]/path/raw?r=1",
            "http host=[::1] host_port=[::1] origin=http://[::1] path=/path query=k=v/raw?r=1 target=/path?k=v/raw?r=1 endpoint=[::1],- url=http://[::1]/path?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://[2001:db8::1]/path?a=b",
        [
            "http host=[2001:db8::1] host_port=[2001:db8::1] origin=http://[2001:db8::1] path=/path query=a=b target=/path?a=b endpoint=[2001:db8::1],- url=http://[2001:db8::1]/path?a=b",
            "http host=[2001:db8::1] host_port=[2001:db8::1] origin=http://[2001:db8::1] path=/path/seg query=a=b&k=v target=/path/seg?a=b&k=v endpoint=[2001:db8::1],- url=http://[2001:db8::1]/path/seg?a=b&k=v",
            "http host=[2001:db8::1] host_port=[2001:db8::1] origin=http://[2001:db8::1] path=/path query=a=b/raw?r=1 target=/path?a=b/raw?r=1 endpoint=[2001:db8::1],- url=http://[2001:db8::1]/path?a=b/raw?r=1",
            "http host=[2001:db8::1] host_port=[2001:db8::1] origin=http://[2001:db8::1] path=/path query=a=b&k=v/raw?r=1 target=/path?a=b&k=v/raw?r=1 endpoint=[2001:db8::1],- url=http://[2001:db8::1]/path?a=b&k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://[::1]:8080/path",
        [
            "http host=[::1] host_port=[::1]:8080 origin=http://[::1]:8080 path=/path query=- target=/path endpoint=[::1],8080 url=http://[::1]:8080/path",
            "http host=[::1] host_port=[::1]:8080 origin=http://[::1]:8080 path=/path/seg query=k=v target=/path/seg?k=v endpoint=[::1],8080 url=http://[::1]:8080/path/seg?k=v",
            "http host=[::1] host_port=[::1]:8080 origin=http://[::1]:8080 path=/path/raw query=r=1 target=/path/raw?r=1 endpoint=[::1],8080 url=http://[::1]:8080/path/raw?r=1",
            "http host=[::1] host_port=[::1]:8080 origin=http://[::1]:8080 path=/path query=k=v/raw?r=1 target=/path?k=v/raw?r=1 endpoint=[::1],8080 url=http://[::1]:8080/path?k=v/raw?r=1",
        ],
    );

    assert_states(
        "[::1]:8080",
        [
            "http host=[::1] host_port=[::1]:8080 origin=http://[::1]:8080 path=/ query=- target=/ endpoint=[::1],8080 url=http://[::1]:8080",
            "http host=[::1] host_port=[::1]:8080 origin=http://[::1]:8080 path=/seg query=k=v target=/seg?k=v endpoint=[::1],8080 url=http://[::1]:8080/seg?k=v",
            "http host=[::1] host_port=[::1]:8080 origin=http://[::1]:8080 path=/raw query=r=1 target=/raw?r=1 endpoint=[::1],8080 url=http://[::1]:8080/raw?r=1",
            "http host=[::1] host_port=[::1]:8080 origin=http://[::1]:8080 path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=[::1],8080 url=http://[::1]:8080?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://[::1",
        [
            "http host=[::1 host_port=[::1 origin=http://[::1 path=/ query=- target=/ endpoint=[::1,- url=http://[::1",
            "http host=[::1 host_port=[::1 origin=http://[::1 path=/seg query=k=v target=/seg?k=v endpoint=[::1,- url=http://[::1/seg?k=v",
            "http host=[::1 host_port=[::1 origin=http://[::1 path=/raw query=r=1 target=/raw?r=1 endpoint=[::1,- url=http://[::1/raw?r=1",
            "http host=[::1 host_port=[::1 origin=http://[::1 path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=[::1,- url=http://[::1?k=v/raw?r=1",
        ],
    );
}

/// Every spelling `Scheme::try_parse` takes for a unix socket, in any case, with any
/// number of slashes after it, names the same socket — a path with one leading slash,
/// the way rust-extensions reads it — and prints back as it was written.
///
/// A path in the home directory keeps no slash, and without a slash after it `unix` is
/// a host name.
#[test]
fn every_spelling_of_the_unix_scheme() {
    assert_states(
        "unix:///var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=unix:///var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=unix:///var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=unix:///var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=unix:///var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "unix://var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=unix://var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=unix://var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=unix://var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=unix://var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "unix:/var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=unix:/var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=unix:/var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=unix:/var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=unix:/var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "unix+http:///var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=unix+http:///var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=unix+http:///var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=unix+http:///var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=unix+http:///var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "HTTP+UNIX://var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=HTTP+UNIX://var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=HTTP+UNIX://var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=HTTP+UNIX://var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=HTTP+UNIX://var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "http+unix:/var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=http+unix:/var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=http+unix:/var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=http+unix:/var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=http+unix:/var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "http+unix://var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=http+unix://var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=http+unix://var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=http+unix://var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=http+unix://var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "http+unix:///var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=http+unix:///var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=http+unix:///var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=http+unix:///var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=http+unix:///var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "unix:///var/run/docker.sock:/containers/json?all=true",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/containers/json query=all=true target=/containers/json?all=true endpoint=/var/run/docker.sock,- url=unix:///var/run/docker.sock:/containers/json?all=true",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/containers/json/seg query=all=true&k=v target=/containers/json/seg?all=true&k=v endpoint=/var/run/docker.sock,- url=unix:///var/run/docker.sock:/containers/json/seg?all=true&k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/containers/json/raw query=all=true&r=1 target=/containers/json/raw?all=true&r=1 endpoint=/var/run/docker.sock,- url=unix:///var/run/docker.sock:/containers/json/raw?all=true&r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/containers/json/raw query=all=true&k=v&r=1 target=/containers/json/raw?all=true&k=v&r=1 endpoint=/var/run/docker.sock,- url=unix:///var/run/docker.sock:/containers/json/raw?all=true&k=v&r=1",
        ],
    );

    assert_states(
        "http+unix:/~/docker.sock",
        [
            "unix host=~/docker.sock host_port=~/docker.sock origin=~/docker.sock path=/ query=- target=/ endpoint=~/docker.sock,- url=http+unix:/~/docker.sock",
            "unix host=~/docker.sock host_port=~/docker.sock origin=~/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=~/docker.sock,- url=http+unix:/~/docker.sock:/seg?k=v",
            "unix host=~/docker.sock host_port=~/docker.sock origin=~/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=~/docker.sock,- url=http+unix:/~/docker.sock:/raw?r=1",
            "unix host=~/docker.sock host_port=~/docker.sock origin=~/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=~/docker.sock,- url=http+unix:/~/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "http+unix:/",
        [
            "unix host=/ host_port=/ origin=/ path=/ query=- target=/ endpoint=/,- url=http+unix:/",
            "unix host=/ host_port=/ origin=/ path=/seg query=k=v target=/seg?k=v endpoint=/,- url=http+unix:/:/seg?k=v",
            "unix host=/ host_port=/ origin=/ path=/raw query=r=1 target=/raw?r=1 endpoint=/,- url=http+unix:/:/raw?r=1",
            "unix host=/ host_port=/ origin=/ path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/,- url=http+unix:/:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "http+unix://",
        [
            "unix host=/ host_port=/ origin=/ path=/ query=- target=/ endpoint=/,- url=http+unix://",
            "unix host=/ host_port=/ origin=/ path=/seg query=k=v target=/seg?k=v endpoint=/,- url=http+unix://:/seg?k=v",
            "unix host=/ host_port=/ origin=/ path=/raw query=r=1 target=/raw?r=1 endpoint=/,- url=http+unix://:/raw?r=1",
            "unix host=/ host_port=/ origin=/ path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/,- url=http+unix://:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "unix:8080",
        [
            "http host=unix host_port=unix:8080 origin=http://unix:8080 path=/ query=- target=/ endpoint=unix,8080 url=http://unix:8080",
            "http host=unix host_port=unix:8080 origin=http://unix:8080 path=/seg query=k=v target=/seg?k=v endpoint=unix,8080 url=http://unix:8080/seg?k=v",
            "http host=unix host_port=unix:8080 origin=http://unix:8080 path=/raw query=r=1 target=/raw?r=1 endpoint=unix,8080 url=http://unix:8080/raw?r=1",
            "http host=unix host_port=unix:8080 origin=http://unix:8080 path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=unix,8080 url=http://unix:8080?k=v/raw?r=1",
        ],
    );
}

/// A path with no scheme: the socket is everything up to the first ':'.
#[test]
fn a_bare_unix_socket_path() {
    assert_states(
        "/var/run/docker.sock",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/ query=- target=/ endpoint=/var/run/docker.sock,- url=/var/run/docker.sock",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=/var/run/docker.sock,- url=/var/run/docker.sock:/seg?k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=/var/run/docker.sock,- url=/var/run/docker.sock:/raw?r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/var/run/docker.sock,- url=/var/run/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "~/docker.sock",
        [
            "unix host=~/docker.sock host_port=~/docker.sock origin=~/docker.sock path=/ query=- target=/ endpoint=~/docker.sock,- url=~/docker.sock",
            "unix host=~/docker.sock host_port=~/docker.sock origin=~/docker.sock path=/seg query=k=v target=/seg?k=v endpoint=~/docker.sock,- url=~/docker.sock:/seg?k=v",
            "unix host=~/docker.sock host_port=~/docker.sock origin=~/docker.sock path=/raw query=r=1 target=/raw?r=1 endpoint=~/docker.sock,- url=~/docker.sock:/raw?r=1",
            "unix host=~/docker.sock host_port=~/docker.sock origin=~/docker.sock path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=~/docker.sock,- url=~/docker.sock:/raw?k=v&r=1",
        ],
    );

    assert_states(
        "/var/run/docker.sock:/containers/json?all=true",
        [
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/containers/json query=all=true target=/containers/json?all=true endpoint=/var/run/docker.sock,- url=/var/run/docker.sock:/containers/json?all=true",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/containers/json/seg query=all=true&k=v target=/containers/json/seg?all=true&k=v endpoint=/var/run/docker.sock,- url=/var/run/docker.sock:/containers/json/seg?all=true&k=v",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/containers/json/raw query=all=true&r=1 target=/containers/json/raw?all=true&r=1 endpoint=/var/run/docker.sock,- url=/var/run/docker.sock:/containers/json/raw?all=true&r=1",
            "unix host=/var/run/docker.sock host_port=/var/run/docker.sock origin=/var/run/docker.sock path=/containers/json/raw query=all=true&k=v&r=1 target=/containers/json/raw?all=true&k=v&r=1 endpoint=/var/run/docker.sock,- url=/var/run/docker.sock:/containers/json/raw?all=true&k=v&r=1",
        ],
    );

    assert_states(
        "/",
        [
            "unix host=/ host_port=/ origin=/ path=/ query=- target=/ endpoint=/,- url=/",
            "unix host=/ host_port=/ origin=/ path=/seg query=k=v target=/seg?k=v endpoint=/,- url=/:/seg?k=v",
            "unix host=/ host_port=/ origin=/ path=/raw query=r=1 target=/raw?r=1 endpoint=/,- url=/:/raw?r=1",
            "unix host=/ host_port=/ origin=/ path=/raw query=k=v&r=1 target=/raw?k=v&r=1 endpoint=/,- url=/:/raw?k=v&r=1",
        ],
    );
}

/// With a query and no path the path is `/`, whether the query was in the address or was
/// appended — and a raw ending after it leaves the host alone.
#[test]
fn a_query_with_no_path() {
    assert_states(
        "http://host?a=b",
        [
            "http host=host host_port=host origin=http://host path=/ query=a=b target=/?a=b endpoint=host,- url=http://host?a=b",
            "http host=host host_port=host origin=http://host path=/seg query=a=b&k=v target=/seg?a=b&k=v endpoint=host,- url=http://host/seg?a=b&k=v",
            "http host=host host_port=host origin=http://host path=/ query=a=b/raw?r=1 target=/?a=b/raw?r=1 endpoint=host,- url=http://host?a=b/raw?r=1",
            "http host=host host_port=host origin=http://host path=/ query=a=b&k=v/raw?r=1 target=/?a=b&k=v/raw?r=1 endpoint=host,- url=http://host?a=b&k=v/raw?r=1",
        ],
    );

    assert_states(
        "https://google.com",
        [
            "https host=google.com host_port=google.com origin=https://google.com path=/ query=- target=/ endpoint=google.com,- url=https://google.com",
            "https host=google.com host_port=google.com origin=https://google.com path=/seg query=k=v target=/seg?k=v endpoint=google.com,- url=https://google.com/seg?k=v",
            "https host=google.com host_port=google.com origin=https://google.com path=/raw query=r=1 target=/raw?r=1 endpoint=google.com,- url=https://google.com/raw?r=1",
            "https host=google.com host_port=google.com origin=https://google.com path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=google.com,- url=https://google.com?k=v/raw?r=1",
        ],
    );

    assert_states(
        "host:8080",
        [
            "http host=host host_port=host:8080 origin=http://host:8080 path=/ query=- target=/ endpoint=host,8080 url=http://host:8080",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/seg query=k=v target=/seg?k=v endpoint=host,8080 url=http://host:8080/seg?k=v",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/raw query=r=1 target=/raw?r=1 endpoint=host,8080 url=http://host:8080/raw?r=1",
            "http host=host host_port=host:8080 origin=http://host:8080 path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=host,8080 url=http://host:8080?k=v/raw?r=1",
        ],
    );
}

/// For comparison with the forms above.
#[test]
fn ordinary_addresses() {
    assert_states(
        "google.com",
        [
            "http host=google.com host_port=google.com origin=http://google.com path=/ query=- target=/ endpoint=google.com,- url=http://google.com",
            "http host=google.com host_port=google.com origin=http://google.com path=/seg query=k=v target=/seg?k=v endpoint=google.com,- url=http://google.com/seg?k=v",
            "http host=google.com host_port=google.com origin=http://google.com path=/raw query=r=1 target=/raw?r=1 endpoint=google.com,- url=http://google.com/raw?r=1",
            "http host=google.com host_port=google.com origin=http://google.com path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=google.com,- url=http://google.com?k=v/raw?r=1",
        ],
    );

    assert_states(
        "localhost:8080/templates",
        [
            "http host=localhost host_port=localhost:8080 origin=http://localhost:8080 path=/templates query=- target=/templates endpoint=localhost,8080 url=http://localhost:8080/templates",
            "http host=localhost host_port=localhost:8080 origin=http://localhost:8080 path=/templates/seg query=k=v target=/templates/seg?k=v endpoint=localhost,8080 url=http://localhost:8080/templates/seg?k=v",
            "http host=localhost host_port=localhost:8080 origin=http://localhost:8080 path=/templates/raw query=r=1 target=/templates/raw?r=1 endpoint=localhost,8080 url=http://localhost:8080/templates/raw?r=1",
            "http host=localhost host_port=localhost:8080 origin=http://localhost:8080 path=/templates query=k=v/raw?r=1 target=/templates?k=v/raw?r=1 endpoint=localhost,8080 url=http://localhost:8080/templates?k=v/raw?r=1",
        ],
    );

    assert_states(
        "https://my-domain:5123/my-path",
        [
            "https host=my-domain host_port=my-domain:5123 origin=https://my-domain:5123 path=/my-path query=- target=/my-path endpoint=my-domain,5123 url=https://my-domain:5123/my-path",
            "https host=my-domain host_port=my-domain:5123 origin=https://my-domain:5123 path=/my-path/seg query=k=v target=/my-path/seg?k=v endpoint=my-domain,5123 url=https://my-domain:5123/my-path/seg?k=v",
            "https host=my-domain host_port=my-domain:5123 origin=https://my-domain:5123 path=/my-path/raw query=r=1 target=/my-path/raw?r=1 endpoint=my-domain,5123 url=https://my-domain:5123/my-path/raw?r=1",
            "https host=my-domain host_port=my-domain:5123 origin=https://my-domain:5123 path=/my-path query=k=v/raw?r=1 target=/my-path?k=v/raw?r=1 endpoint=my-domain,5123 url=https://my-domain:5123/my-path?k=v/raw?r=1",
        ],
    );

    assert_states(
        "HTTP://host",
        [
            "http host=host host_port=host origin=HTTP://host path=/ query=- target=/ endpoint=host,- url=HTTP://host",
            "http host=host host_port=host origin=HTTP://host path=/seg query=k=v target=/seg?k=v endpoint=host,- url=HTTP://host/seg?k=v",
            "http host=host host_port=host origin=HTTP://host path=/raw query=r=1 target=/raw?r=1 endpoint=host,- url=HTTP://host/raw?r=1",
            "http host=host host_port=host origin=HTTP://host path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=host,- url=HTTP://host?k=v/raw?r=1",
        ],
    );

    assert_states(
        "wss://host/ws",
        [
            "wss host=host host_port=host origin=wss://host path=/ws query=- target=/ws endpoint=host,- url=wss://host/ws",
            "wss host=host host_port=host origin=wss://host path=/ws/seg query=k=v target=/ws/seg?k=v endpoint=host,- url=wss://host/ws/seg?k=v",
            "wss host=host host_port=host origin=wss://host path=/ws/raw query=r=1 target=/ws/raw?r=1 endpoint=host,- url=wss://host/ws/raw?r=1",
            "wss host=host host_port=host origin=wss://host path=/ws query=k=v/raw?r=1 target=/ws?k=v/raw?r=1 endpoint=host,- url=wss://host/ws?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://хост.рф/путь?q=я",
        [
            "http host=хост.рф host_port=хост.рф origin=http://хост.рф path=/путь query=q=я target=/путь?q=я endpoint=хост.рф,- url=http://хост.рф/путь?q=я",
            "http host=хост.рф host_port=хост.рф origin=http://хост.рф path=/путь/seg query=q=я&k=v target=/путь/seg?q=я&k=v endpoint=хост.рф,- url=http://хост.рф/путь/seg?q=я&k=v",
            "http host=хост.рф host_port=хост.рф origin=http://хост.рф path=/путь query=q=я/raw?r=1 target=/путь?q=я/raw?r=1 endpoint=хост.рф,- url=http://хост.рф/путь?q=я/raw?r=1",
            "http host=хост.рф host_port=хост.рф origin=http://хост.рф path=/путь query=q=я&k=v/raw?r=1 target=/путь?q=я&k=v/raw?r=1 endpoint=хост.рф,- url=http://хост.рф/путь?q=я&k=v/raw?r=1",
        ],
    );
}

/// `#` is not a delimiter for the builder: a fragment stays in whatever it follows, the
/// host or the path. `RemoteEndpoint` does end the host at it, so for `http://host#frag`
/// the builder and its endpoint name different hosts.
///
/// Pinned as it is. Whether to cut it off, to refuse it or to leave it is not decided.
#[test]
fn a_fragment_is_not_recognised() {
    assert_states(
        "http://host#frag",
        [
            "http host=host#frag host_port=host#frag origin=http://host#frag path=/ query=- target=/ endpoint=host,- url=http://host#frag",
            "http host=host#frag host_port=host#frag origin=http://host#frag path=/seg query=k=v target=/seg?k=v endpoint=host,- url=http://host#frag/seg?k=v",
            "http host=host#frag host_port=host#frag origin=http://host#frag path=/raw query=r=1 target=/raw?r=1 endpoint=host,- url=http://host#frag/raw?r=1",
            "http host=host#frag host_port=host#frag origin=http://host#frag path=/ query=k=v/raw?r=1 target=/?k=v/raw?r=1 endpoint=host,- url=http://host#frag?k=v/raw?r=1",
        ],
    );

    assert_states(
        "http://host/path#frag",
        [
            "http host=host host_port=host origin=http://host path=/path#frag query=- target=/path#frag endpoint=host,- url=http://host/path#frag",
            "http host=host host_port=host origin=http://host path=/path#frag/seg query=k=v target=/path#frag/seg?k=v endpoint=host,- url=http://host/path#frag/seg?k=v",
            "http host=host host_port=host origin=http://host path=/path#frag/raw query=r=1 target=/path#frag/raw?r=1 endpoint=host,- url=http://host/path#frag/raw?r=1",
            "http host=host host_port=host origin=http://host path=/path#frag query=k=v/raw?r=1 target=/path#frag?k=v/raw?r=1 endpoint=host,- url=http://host/path#frag?k=v/raw?r=1",
        ],
    );
}

/// After a query the two builders treat a raw ending differently: the tcp one appends it
/// to the query as it is, the unix one splits it into the path and the query.
///
/// Pinned as it is. Whether to make them the same is not decided.
#[test]
fn a_raw_ending_after_a_query() {
    assert_states(
        "http://host/base?x=1",
        [
            "http host=host host_port=host origin=http://host path=/base query=x=1 target=/base?x=1 endpoint=host,- url=http://host/base?x=1",
            "http host=host host_port=host origin=http://host path=/base/seg query=x=1&k=v target=/base/seg?x=1&k=v endpoint=host,- url=http://host/base/seg?x=1&k=v",
            "http host=host host_port=host origin=http://host path=/base query=x=1/raw?r=1 target=/base?x=1/raw?r=1 endpoint=host,- url=http://host/base?x=1/raw?r=1",
            "http host=host host_port=host origin=http://host path=/base query=x=1&k=v/raw?r=1 target=/base?x=1&k=v/raw?r=1 endpoint=host,- url=http://host/base?x=1&k=v/raw?r=1",
        ],
    );

    assert_states(
        "/sock:/base?x=1",
        [
            "unix host=/sock host_port=/sock origin=/sock path=/base query=x=1 target=/base?x=1 endpoint=/sock,- url=/sock:/base?x=1",
            "unix host=/sock host_port=/sock origin=/sock path=/base/seg query=x=1&k=v target=/base/seg?x=1&k=v endpoint=/sock,- url=/sock:/base/seg?x=1&k=v",
            "unix host=/sock host_port=/sock origin=/sock path=/base/raw query=x=1&r=1 target=/base/raw?x=1&r=1 endpoint=/sock,- url=/sock:/base/raw?x=1&r=1",
            "unix host=/sock host_port=/sock origin=/sock path=/base/raw query=x=1&k=v&r=1 target=/base/raw?x=1&k=v&r=1 endpoint=/sock,- url=/sock:/base/raw?x=1&k=v&r=1",
        ],
    );
}

#[test]
fn an_ipv6_literal_is_an_ip_with_a_port_and_without() {
    for src in [
        "http://[::1]",
        "http://[::1]/path",
        "http://[::1]:8080",
        "[2001:db8::1]",
    ] {
        assert!(UrlBuilder::new(src).host_is_ip(), "{src}");
    }

    assert!(!UrlBuilder::new("http://[host]").host_is_ip());
}

/// Pieces an address is put together from: every delimiter the builder looks at, the
/// schemes it knows, and what goes between them.
const PIECES: [&str; 14] = [
    "http", "unix", "host", "8080", "é", ":", "/", "://", "?", "#", "[", "]", "~", "&",
];

/// Every address made of up to `len` of the pieces above.
fn addresses(len: usize) -> Vec<String> {
    let mut result = vec![String::new()];
    let mut from = 0;

    for _ in 0..len {
        let to = result.len();

        for index in from..to {
            for piece in PIECES {
                result.push(format!("{}{}", result[index], piece));
            }
        }

        from = to;
    }

    result
}

fn append(url: &mut UrlBuilder, what: usize) {
    match what {
        0 => url.append_path_segment("/a b?c#d"),
        1 => url.append_query_param("k", Some("v")),
        2 => url.append_raw_ending("/raw?r=1"),
        3 => url.append_raw_ending("raw"),
        4 => url.append_raw_ending("?r=1"),
        _ => url.append_raw_ending(""),
    }
}

const APPENDS: usize = 6;

/// Calls every getter, and checks what has to hold for any address at all.
fn assert_is_sound(url: &UrlBuilder, src: &str) {
    let host = url.get_host();
    let host_port = url.get_host_port();
    let path = url.get_path();
    let query = url.get_query();
    let path_and_query = url.get_path_and_query();
    let text = url.to_string();

    let _ = url.get_scheme();
    let _ = url.host_is_ip();
    let _ = url.iter_query().map(|query| query.count());

    let endpoint = url.get_remote_endpoint(Some(80));
    let _ = endpoint.get_host();
    let _ = endpoint.get_port_str();
    let _ = endpoint.get_port();

    let expected_path_and_query = match query {
        Some(query) => format!("{path}?{query}"),
        None => path.to_string(),
    };
    assert_eq!(
        path_and_query, expected_path_and_query,
        "{src:?} -> {text:?}"
    );

    if url.is_unix_socket() {
        assert_eq!(host, host_port, "{src:?} -> {text:?}");
        assert_eq!(host, url.get_scheme_and_host(), "{src:?} -> {text:?}");
        return;
    }

    // The host ends where the port, the path or the query starts.
    assert!(!host_port.contains(['/', '?']), "{src:?} -> {text:?}");
    assert!(host_port.starts_with(host), "{src:?} -> {text:?}");
    assert!(
        host_port[host.len()..].is_empty() || host_port[host.len()..].starts_with(':'),
        "{src:?} -> {text:?}"
    );
    assert!(
        url.get_scheme_and_host().ends_with(host_port),
        "{src:?} -> {text:?}"
    );
    assert!(
        text.starts_with(url.get_scheme_and_host()),
        "{src:?} -> {text:?}"
    );

    // The request target is origin-form: it starts with '/'.
    assert!(path.starts_with('/'), "{src:?} -> {text:?}");
    assert!(!path.contains('?'), "{src:?} -> {text:?}");
}

/// No getter panics on any address, and no append moves the host.
#[test]
fn no_address_makes_a_getter_panic() {
    for src in addresses(4) {
        assert_is_sound(&UrlBuilder::new(&src), &src);
    }

    for src in addresses(3) {
        let host_port = UrlBuilder::new(&src).get_host_port().to_string();

        for first in 0..APPENDS {
            for second in 0..APPENDS {
                let mut url = UrlBuilder::new(&src);

                append(&mut url, first);
                assert_is_sound(&url, &src);

                append(&mut url, second);
                assert_is_sound(&url, &src);

                assert_eq!(url.get_host_port(), host_port, "{src:?} -> {url}");
            }
        }
    }
}

#[test]
fn no_three_appends_in_a_row_make_a_getter_panic() {
    for src in addresses(2) {
        for first in 0..APPENDS {
            for second in 0..APPENDS {
                for third in 0..APPENDS {
                    let mut url = UrlBuilder::new(&src);

                    for what in [first, second, third] {
                        append(&mut url, what);
                        assert_is_sound(&url, &src);
                    }
                }
            }
        }
    }
}

/// `UrlBuilder::new` picks one of the two builders by the scheme. Both are public, so
/// each is also given the addresses meant for the other.
#[test]
fn no_address_makes_a_builder_made_directly_panic() {
    for src in addresses(4) {
        for append_first in [false, true] {
            let mut tcp = UrlBuilderInner::new(&src);
            let mut unix = UrlBuilderUnixSocket::new(&src);

            if append_first {
                tcp.append_query_param("k", Some("v"));
                tcp.append_path_segment("seg");
                tcp.append_raw_ending("/raw?r=1");

                unix.append_query_param("k", Some("v"));
                unix.append_path_segment("seg");
                unix.append_raw_ending("/raw?r=1");
            }

            let _ = tcp.get_scheme();
            let _ = tcp.get_host();
            let _ = tcp.get_host_port();
            let _ = tcp.get_scheme_and_host();
            let _ = tcp.get_path();
            let _ = tcp.get_query();
            let _ = tcp.get_path_and_query();
            let _ = tcp.host_is_ip();
            let _ = tcp.get_remote_endpoint(Some(80)).get_host();

            let _ = unix.get_host();
            let _ = unix.get_scheme_and_host();
            let _ = unix.get_path();
            let _ = unix.get_query();
            let _ = unix.get_path_and_query();
            let _ = unix.get_remote_endpoint().get_host();
            let _ = unix.to_string();
        }
    }
}
