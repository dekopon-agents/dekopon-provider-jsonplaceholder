#![allow(missing_docs)]

use dekopon_jsonplaceholder_provider::JsonPlaceholder;
use dekopon_provider_sdk::provider::{Header, Response};
use dekopon_provider_sdk_testkit::{HttpScript, Native};
use serde_json::{Value, json};

#[test]
fn authored_cassette_replays_with_exact_owner_prefix_and_default_routing() {
    let exchange: Value = serde_json::from_str(include_str!(
        "cassettes/jsonplaceholder/0001-GET-posts-7.json"
    ))
    .unwrap();
    assert_eq!(exchange["version"], 1);
    assert!(exchange["request"]["query"].is_null());
    for (settings, host, base) in [
        (
            None,
            "jsonplaceholder.typicode.com",
            "https://jsonplaceholder.typicode.com",
        ),
        (
            Some(json!({})),
            "jsonplaceholder.typicode.com",
            "https://jsonplaceholder.typicode.com",
        ),
        (
            Some(json!({"baseUrl": "https://fixture.example.test/jsonplaceholder/v1/"})),
            "fixture.example.test",
            "https://fixture.example.test/jsonplaceholder/v1",
        ),
        (
            Some(json!({"baseUrl": "https://127.0.0.1:43123/fixture/"})),
            "127.0.0.1:43123",
            "https://127.0.0.1:43123/fixture",
        ),
    ] {
        let mut native = Native::<JsonPlaceholder>::new().http(HttpScript::new(
            host,
            "GET",
            Response {
                status: exchange["response"]["status"]
                    .as_u64()
                    .unwrap()
                    .try_into()
                    .unwrap(),
                headers: vec![
                    Header::text(
                        "content-type",
                        exchange["response"]["headers"]["content-type"]
                            .as_str()
                            .unwrap(),
                    )
                    .unwrap(),
                ],
                body: serde_json::to_vec(&exchange["response"]["body"]["json"]).unwrap(),
            },
        ));
        if let Some(settings) = settings {
            native = native.settings(settings);
        }
        let output = native.call("jsonplaceholder.posts.get", r#"{"postId":7}"#);
        assert_eq!(output.status, 0, "{}", output.stderr);
        let sent = native.requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].method, exchange["request"]["method"]);
        assert_eq!(
            sent[0].uri,
            format!("{base}{}", exchange["request"]["path"].as_str().unwrap())
        );
        assert!(sent[0].body.is_empty());
        assert_eq!(
            sent[0].headers,
            vec![
                Header::text(
                    "accept",
                    exchange["request"]["headers"]["accept"].as_str().unwrap()
                )
                .unwrap()
            ]
        );
        assert!(
            !sent[0]
                .headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("authorization"))
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"post": exchange["response"]["body"]["json"]})
        );
    }
}
