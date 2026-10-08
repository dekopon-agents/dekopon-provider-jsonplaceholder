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
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("one JSON object"),
        json!({"post": {"userId": 2, "id": 7, "title": "mock", "body": "fixture"}})
    );
    assert_eq!(
        output.stdout.iter().filter(|&&byte| byte == b'\n').count(),
        1
    );
    assert!(output.stdout.ends_with(b"\n"));
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

#[test]
fn settings_and_model_origin_controls_fail_before_requests() {
    for (capability, input) in [
        ("jsonplaceholder.posts.get", json!({"postId": 7})),
        (
            "jsonplaceholder.posts.create",
            json!({"userId": 3, "title": "t", "body": "b"}),
        ),
    ] {
        for settings in [
            json!({"baseUrl": "https://fixture.example.test/?q=1"}),
            json!({"baseUrl": "https://user@fixture.example.test"}),
            json!({"baseUrl": "https://fixture.example.test/#fragment"}),
            json!({"baseUrl": "ftp://fixture.example.test"}),
            json!({"baseUrl": "fixture.example.test"}),
            json!({"baseUrl": "https://"}),
            json!({"baseUrl": "https://fixture.example.test/ space"}),
            json!({"baseUrl": 42}),
            json!({"endpoint": "http://127.0.0.1:43123"}),
        ] {
            let native = Native::<JsonPlaceholder>::new().settings(settings);
            let output = native.call(capability, &input.to_string());
            assert_ne!(output.status, 0);
            assert!(output.stderr.contains("settings"), "{}", output.stderr);
            assert!(native.requests().is_empty());
        }
        for field in ["endpoint", "baseUrl", "url"] {
            let mut input = input.clone();
            input[field] = json!("http://127.0.0.1:43123");
            let native = Native::<JsonPlaceholder>::new();
            let output = native.call(capability, &input.to_string());
            assert_ne!(output.status, 0);
            assert!(native.requests().is_empty());
        }
    }
}

#[test]
fn create_preserves_default_and_owner_prefixed_routing() {
    for (settings, host, uri) in [
        (
            None,
            "jsonplaceholder.typicode.com",
            "https://jsonplaceholder.typicode.com/posts",
        ),
        (
            Some(json!({"baseUrl": "https://[::1]:43124/fixture/"})),
            "[::1]:43124",
            "https://[::1]:43124/fixture/posts",
        ),
    ] {
        let mut native = Native::<JsonPlaceholder>::new().http(HttpScript::new(
            host,
            "POST",
            Response {
                status: 201,
                headers: vec![],
                body: br#"{"userId":3,"id":101,"title":"t","body":"b"}"#.to_vec(),
            },
        ));
        if let Some(settings) = settings {
            native = native.settings(settings);
        }
        let output = native.call(
            "jsonplaceholder.posts.create",
            r#"{"userId":3,"title":"t","body":"b"}"#,
        );
        assert_eq!(output.status, 0, "{}", output.stderr);
        let sent = native.requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].uri, uri);
        assert_eq!(sent[0].method, "POST");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&sent[0].body).unwrap(),
            json!({"userId":3,"title":"t","body":"b"})
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            json!({"post":{"userId":3,"id":101,"title":"t","body":"b"}})
        );
    }
}

#[test]
fn real_component_rejects_old_model_endpoint_and_invalid_settings() {
    use dekopon_provider_sdk_testkit::Harness;
    let component =
        std::env::var_os("DEKOPON_PROVIDER_COMPONENT").expect("built component required");
    for (capability, input) in [
        ("jsonplaceholder.posts.get", json!({"postId": 7})),
        (
            "jsonplaceholder.posts.create",
            json!({"userId": 3, "title": "t", "body": "b"}),
        ),
    ] {
        let mut old_input = input.clone();
        old_input["endpoint"] = json!("http://127.0.0.1:43123");
        let refused = Harness::<JsonPlaceholder>::get(&component)
            .call(capability, old_input)
            .unwrap();
        assert_ne!(refused.status, 0);
        assert!(refused.http_calls.is_empty());
        let refused = Harness::<JsonPlaceholder>::get(&component)
            .settings(json!({"baseUrl": "https://user@fixture.example.test"}))
            .call(capability, input)
            .unwrap();
        assert_ne!(refused.status, 0);
        assert!(refused.stderr.contains("settings"));
        assert!(refused.http_calls.is_empty());
    }
}

#[test]
fn removed_endpoint_flags_are_usage_errors() {
    use dekopon_provider_sdk::{CommandRunOutcome, provider};
    for args in [
        vec![
            "posts",
            "get",
            "--post-id",
            "7",
            "--endpoint",
            "http://127.0.0.1:43123",
        ],
        vec![
            "posts",
            "create",
            "--user-id",
            "3",
            "--title",
            "t",
            "--body",
            "b",
            "--endpoint",
            "http://127.0.0.1:43123",
        ],
    ] {
        let result = provider::command::<JsonPlaceholder>(
            &args.into_iter().map(str::to_owned).collect::<Vec<_>>(),
            false,
        );
        assert!(matches!(
            result,
            CommandRunOutcome::Rendered { status: 2, .. }
        ));
    }
}
