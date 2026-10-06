//! Typed native and real-component conformance; fixtures never contact public services.
use dekopon_jsonplaceholder_provider::JsonPlaceholder;
use dekopon_provider_sdk::provider::Response;
use dekopon_provider_sdk_testkit::{HttpScript, Native, conformance};
use serde_json::json;

#[test]
fn real_component_conforms_to_typed_manifest_and_imports() {
    let component =
        std::env::var_os("DEKOPON_PROVIDER_COMPONENT").expect("built component required");
    conformance::<JsonPlaceholder>(component).expect("HTTP and stdio imports, no WASI");
}

#[test]
fn native_get_emits_exact_json_line_and_no_error() {
    let native = Native::<JsonPlaceholder>::new().http(HttpScript::new(
        "jsonplaceholder.typicode.com",
        "GET",
        Response {
            status: 200,
            headers: vec![],
            body: serde_json::to_vec(&json!({
                "userId": 2, "id": 7, "title": "mock", "body": "fixture"
            }))
            .unwrap(),
        },
    ));
    let output = native.call("jsonplaceholder.posts.get", r#"{"postId":7}"#);
    assert_eq!(output.status, 0, "{}", output.stderr);
    assert_eq!(
        output.stdout,
        b"{\"post\":{\"body\":\"fixture\",\"id\":7,\"title\":\"mock\",\"userId\":2}}\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn piped_body_is_bounded_and_read_only_on_authorized_invoke() {
    let native = Native::<JsonPlaceholder>::new()
        .stdin(b"private-piped-canary".to_vec())
        .http(HttpScript::new(
            "jsonplaceholder.typicode.com",
            "POST",
            Response {
                status: 201,
                headers: vec![],
                body: serde_json::to_vec(&json!({
                    "userId": 3, "id": 101, "title": "t", "body": "private-piped-canary"
                }))
                .unwrap(),
            },
        ));
    let output = native.call(
        "jsonplaceholder.posts.create",
        r#"{"userId":3,"title":"t","body":"-","stdinPiped":true}"#,
    );
    assert_eq!(output.status, 0, "{}", output.stderr);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["post"]["body"],
        "private-piped-canary"
    );
    assert!(output.stdout.ends_with(b"\n"));
    let oversized = Native::<JsonPlaceholder>::new()
        .stdin(vec![b'x'; 4097])
        .call(
            "jsonplaceholder.posts.create",
            r#"{"userId":3,"title":"t","body":"-","stdinPiped":true}"#,
        );
    assert_ne!(oversized.status, 0);
    assert!(oversized.stdout.is_empty());
}
