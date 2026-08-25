//! Bounded typed JSONPlaceholder capabilities over broker-mediated HTTP.

use std::net::SocketAddr;

use dekopon_provider_http::{Header, HttpError, Request, Response, method};
use dekopon_provider_sdk::{
    CapabilityId, EffectKind, Idempotency, Provider, ProviderApiVersion, ProviderCapability,
    ProviderError, ProviderManifest, RiskLevel,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DEFAULT_ENDPOINT: &str = "https://jsonplaceholder.typicode.com";
const PRODUCTION_HOST: &str = "jsonplaceholder.typicode.com";
const MAX_ENDPOINT_BYTES: usize = 512;
const MAX_TITLE_BYTES: usize = 256;
const MAX_BODY_BYTES: usize = 4 * 1024;
const MAX_RESPONSE_TITLE_BYTES: usize = 4 * 1024;
const MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024;

mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "provider",
        generate_all,
        pub_export_macro: true,
    });
}

/// The exact dependency notices are embedded in the shipped component bytes.
#[cfg(target_arch = "wasm32")]
#[used]
#[unsafe(link_section = "dekopon.third-party-notices")]
static THIRD_PARTY_NOTICES: [u8; include_bytes!("../THIRD_PARTY_NOTICES.md").len()] =
    *include_bytes!("../THIRD_PARTY_NOTICES.md");

struct JsonPlaceholder;

impl Provider for JsonPlaceholder {
    fn manifest() -> ProviderManifest {
        ProviderManifest {
            api_version: ProviderApiVersion::V1Alpha1,
            id: "jsonplaceholder"
                .parse()
                .expect("static provider ID is valid"),
            description: "Reads and creates bounded JSONPlaceholder posts through broker HTTP"
                .to_owned(),
            command_words: Vec::new(),
            capabilities: vec![
                ProviderCapability {
                    id: "jsonplaceholder.posts.get"
                        .parse()
                        .expect("static capability ID is valid"),
                    description: "Gets one JSONPlaceholder post by numeric ID".to_owned(),
                    effect: EffectKind::ReadOnly,
                    risk: RiskLevel::Low,
                    idempotency: Idempotency::Idempotent,
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "postId": {"type": "integer", "minimum": 1, "maximum": 100},
                            "endpoint": {
                                "type": "string",
                                "maxLength": MAX_ENDPOINT_BYTES,
                                "x-dekopon-maxUtf8Bytes": MAX_ENDPOINT_BYTES,
                                "description": "Optional broker-constrained endpoint, limited to 512 UTF-8 bytes; defaults to JSONPlaceholder. Plain HTTP accepts only literal loopback test endpoints."
                            }
                        },
                        "required": ["postId"],
                        "additionalProperties": false
                    }),
                },
                ProviderCapability {
                    id: "jsonplaceholder.posts.create"
                        .parse()
                        .expect("static capability ID is valid"),
                    description: "Creates one non-persistent JSONPlaceholder post".to_owned(),
                    effect: EffectKind::ExternalWrite,
                    risk: RiskLevel::Medium,
                    idempotency: Idempotency::NonIdempotent,
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "userId": {"type": "integer", "minimum": 1, "maximum": 10},
                            "title": {
                                "type": "string",
                                "minLength": 1,
                                "maxLength": MAX_TITLE_BYTES,
                                "x-dekopon-maxUtf8Bytes": MAX_TITLE_BYTES,
                                "description": "Post title, limited to 256 UTF-8 bytes."
                            },
                            "body": {
                                "type": "string",
                                "minLength": 1,
                                "maxLength": MAX_BODY_BYTES,
                                "x-dekopon-maxUtf8Bytes": MAX_BODY_BYTES,
                                "description": "Post body, limited to 4096 UTF-8 bytes."
                            },
                            "endpoint": {
                                "type": "string",
                                "maxLength": MAX_ENDPOINT_BYTES,
                                "x-dekopon-maxUtf8Bytes": MAX_ENDPOINT_BYTES,
                                "description": "Optional broker-constrained endpoint, limited to 512 UTF-8 bytes; defaults to JSONPlaceholder. Plain HTTP accepts only literal loopback test endpoints."
                            }
                        },
                        "required": ["userId", "title", "body"],
                        "additionalProperties": false
                    }),
                },
            ],
        }
    }

    fn invoke(capability: &CapabilityId, input: Value) -> Result<Value, ProviderError> {
        invoke_with(capability, input, dekopon_provider_http::send)
    }
}

fn invoke_with<F>(capability: &CapabilityId, input: Value, send: F) -> Result<Value, ProviderError>
where
    F: FnOnce(Request) -> Result<Response, HttpError>,
{
    match capability.as_str() {
        "jsonplaceholder.posts.get" => get_post(input, send),
        "jsonplaceholder.posts.create" => create_post(input, send),
        _ => Err(ProviderError::new(
            "unknown-capability",
            "unsupported JSONPlaceholder capability",
        )),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct GetPostInput {
    post_id: u32,
    #[serde(default)]
    endpoint: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CreatePostInput {
    user_id: u32,
    title: String,
    body: String,
    #[serde(default)]
    endpoint: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Post {
    user_id: u32,
    id: u32,
    title: String,
    body: String,
}

fn get_post<F>(input: Value, send: F) -> Result<Value, ProviderError>
where
    F: FnOnce(Request) -> Result<Response, HttpError>,
{
    let input = serde_json::from_value::<GetPostInput>(input).map_err(|_| invalid_input())?;
    if !(1..=100).contains(&input.post_id) {
        return Err(invalid_input());
    }
    let endpoint = endpoint(input.endpoint.as_deref())?;
    let request = Request::new(method::GET, format!("{endpoint}/posts/{}", input.post_id))
        .map_err(|_| invalid_request())?
        .with_header(json_header("accept")?);
    let response = send(request).map_err(|_| http_failed())?;
    if response.status == 404 {
        return Err(ProviderError::new("not-found", "post was not found"));
    }
    if response.status != 200 {
        return Err(unexpected_status());
    }
    let post = decode_post(&response.body)?;
    if post.id != input.post_id {
        return Err(invalid_response());
    }
    Ok(json!({"post": post}))
}

fn create_post<F>(input: Value, send: F) -> Result<Value, ProviderError>
where
    F: FnOnce(Request) -> Result<Response, HttpError>,
{
    let input = serde_json::from_value::<CreatePostInput>(input).map_err(|_| invalid_input())?;
    validate_create_input(&input)?;
    let endpoint = endpoint(input.endpoint.as_deref())?;
    let body = serde_json::to_vec(&json!({
        "userId": input.user_id,
        "title": &input.title,
        "body": &input.body
    }))
    .map_err(|_| invalid_request())?;
    let request = Request::new(method::POST, format!("{endpoint}/posts"))
        .map_err(|_| invalid_request())?
        .with_header(json_header("accept")?)
        .with_header(json_header("content-type")?)
        .with_body(body);
    let response = send(request).map_err(|_| http_failed())?;
    if response.status != 201 {
        return Err(unexpected_status());
    }
    let post = decode_post(&response.body)?;
    if post.id == 0
        || post.user_id != input.user_id
        || post.title != input.title
        || post.body != input.body
    {
        return Err(invalid_response());
    }
    Ok(json!({"post": post}))
}

fn validate_create_input(input: &CreatePostInput) -> Result<(), ProviderError> {
    if !(1..=10).contains(&input.user_id)
        || input.title.is_empty()
        || input.title.len() > MAX_TITLE_BYTES
        || input.body.is_empty()
        || input.body.len() > MAX_BODY_BYTES
    {
        return Err(invalid_input());
    }
    Ok(())
}

fn decode_post(body: &[u8]) -> Result<Post, ProviderError> {
    let post = serde_json::from_slice::<Post>(body).map_err(|_| invalid_response())?;
    if post.id == 0
        || !(1..=10).contains(&post.user_id)
        || post.title.is_empty()
        || post.title.len() > MAX_RESPONSE_TITLE_BYTES
        || post.body.is_empty()
        || post.body.len() > MAX_RESPONSE_BODY_BYTES
    {
        return Err(invalid_response());
    }
    Ok(post)
}

fn endpoint(value: Option<&str>) -> Result<String, ProviderError> {
    let value = value.unwrap_or(DEFAULT_ENDPOINT);
    if value.len() > MAX_ENDPOINT_BYTES {
        return Err(invalid_endpoint());
    }
    if matches!(
        value,
        "https://jsonplaceholder.typicode.com" | "https://jsonplaceholder.typicode.com/"
    ) {
        return Ok(format!("https://{PRODUCTION_HOST}"));
    }
    let authority = value.strip_prefix("http://").ok_or_else(invalid_endpoint)?;
    let authority = authority.strip_suffix('/').unwrap_or(authority);
    let address = authority
        .parse::<SocketAddr>()
        .map_err(|_| invalid_endpoint())?;
    if address.port() == 0 || !address.ip().is_loopback() {
        return Err(invalid_endpoint());
    }
    Ok(format!("http://{address}"))
}

fn json_header(name: &'static str) -> Result<Header, ProviderError> {
    Header::text(name, "application/json").map_err(|_| invalid_request())
}

fn invalid_input() -> ProviderError {
    ProviderError::new(
        "invalid-input",
        "input does not match the capability contract",
    )
}

fn invalid_endpoint() -> ProviderError {
    ProviderError::new(
        "invalid-endpoint",
        "endpoint must be production JSONPlaceholder HTTPS or explicit loopback HTTP",
    )
}

fn invalid_request() -> ProviderError {
    ProviderError::new(
        "invalid-request",
        "could not construct bounded HTTP request",
    )
}

fn http_failed() -> ProviderError {
    ProviderError::new("http-failed", "broker HTTP request failed")
}

fn unexpected_status() -> ProviderError {
    ProviderError::new(
        "unexpected-status",
        "endpoint returned an unexpected status",
    )
}

fn invalid_response() -> ProviderError {
    ProviderError::new("invalid-response", "endpoint returned an invalid post")
}

dekopon_provider_sdk::export_provider_with_bindings!(JsonPlaceholder, bindings);

#[cfg(test)]
mod tests {
    use dekopon_provider_http::{Header, HttpErrorCode, Response};
    use dekopon_provider_sdk::{EffectKind, Idempotency, Provider, RiskLevel};
    use serde_json::{Value, json};

    use super::{JsonPlaceholder, endpoint, invoke_with};

    fn capability(value: &str) -> dekopon_provider_sdk::CapabilityId {
        value.parse().expect("valid capability fixture")
    }

    #[test]
    fn manifest_separates_read_and_external_write_authority() {
        let manifest = JsonPlaceholder::manifest();
        assert_eq!(manifest.id.as_str(), "jsonplaceholder");
        assert_eq!(manifest.capabilities.len(), 2);
        assert_eq!(manifest.capabilities[0].effect, EffectKind::ReadOnly);
        assert_eq!(manifest.capabilities[0].risk, RiskLevel::Low);
        assert_eq!(
            manifest.capabilities[0].idempotency,
            Idempotency::Idempotent
        );
        assert_eq!(manifest.capabilities[1].effect, EffectKind::ExternalWrite);
        assert_eq!(manifest.capabilities[1].risk, RiskLevel::Medium);
        assert_eq!(
            manifest.capabilities[1].idempotency,
            Idempotency::NonIdempotent
        );
    }

    #[test]
    fn get_uses_only_the_bounded_post_path_and_parses_mock_response() {
        let output = invoke_with(
            &capability("jsonplaceholder.posts.get"),
            json!({"postId": 7, "endpoint": "http://127.0.0.1:43123"}),
            |request| {
                assert_eq!(request.method, "GET");
                assert_eq!(request.uri, "http://127.0.0.1:43123/posts/7");
                assert_eq!(
                    request.headers,
                    vec![Header::text("accept", "application/json").expect("fixed header")]
                );
                assert!(request.body.is_empty());
                Ok(Response {
                    status: 200,
                    headers: Vec::new(),
                    body: serde_json::to_vec(&json!({
                        "userId": 2,
                        "id": 7,
                        "title": "mock title",
                        "body": "mock body"
                    }))
                    .expect("mock response serializes"),
                })
            },
        )
        .expect("mock get succeeds");
        assert_eq!(output["post"]["id"], 7);
        assert_eq!(output["post"]["title"], "mock title");
    }

    #[test]
    fn create_uses_post_json_and_validates_mock_echo() {
        let output = invoke_with(
            &capability("jsonplaceholder.posts.create"),
            json!({
                "userId": 3,
                "title": "created title",
                "body": "created body",
                "endpoint": "http://[::1]:43124"
            }),
            |request| {
                assert_eq!(request.method, "POST");
                assert_eq!(request.uri, "http://[::1]:43124/posts");
                assert_eq!(
                    request.headers,
                    vec![
                        Header::text("accept", "application/json").expect("fixed header"),
                        Header::text("content-type", "application/json").expect("fixed header"),
                    ]
                );
                assert_eq!(
                    serde_json::from_slice::<Value>(&request.body).expect("request body is JSON"),
                    json!({"userId": 3, "title": "created title", "body": "created body"})
                );
                Ok(Response {
                    status: 201,
                    headers: Vec::new(),
                    body: serde_json::to_vec(&json!({
                        "userId": 3,
                        "id": 101,
                        "title": "created title",
                        "body": "created body"
                    }))
                    .expect("mock response serializes"),
                })
            },
        )
        .expect("mock create succeeds");
        assert_eq!(output["post"]["id"], 101);
    }

    #[test]
    fn endpoints_and_inputs_fail_closed() {
        assert_eq!(
            endpoint(None).expect("default endpoint is valid"),
            "https://jsonplaceholder.typicode.com"
        );
        for denied in [
            "http://jsonplaceholder.typicode.com:80",
            "https://example.com",
            "https://user@jsonplaceholder.typicode.com",
            "https://jsonplaceholder.typicode.com/posts",
            "http://127.0.0.1",
        ] {
            assert!(endpoint(Some(denied)).is_err(), "accepted {denied}");
        }
        let error = invoke_with(
            &capability("jsonplaceholder.posts.get"),
            json!({"postId": 0}),
            |_| unreachable!("invalid input must not call HTTP"),
        )
        .expect_err("invalid ID must fail");
        assert_eq!(error.code(), "invalid-input");
    }

    #[test]
    fn host_failure_does_not_expose_transport_detail() {
        let error = invoke_with(
            &capability("jsonplaceholder.posts.get"),
            json!({"postId": 1}),
            |request| {
                assert_eq!(request.uri, "https://jsonplaceholder.typicode.com/posts/1");
                Err(dekopon_provider_http::HttpError {
                    code: HttpErrorCode::Denied,
                    message: "secret path and credential".to_owned(),
                })
            },
        )
        .expect_err("host denial must fail");
        assert_eq!(error.code(), "http-failed");
        assert_eq!(error.message(), "broker HTTP request failed");
    }

    #[test]
    fn endpoint_allowlist_accepts_only_production_or_literal_loopback_sockets() {
        for (input, normalized) in [
            (
                "https://jsonplaceholder.typicode.com",
                "https://jsonplaceholder.typicode.com",
            ),
            (
                "https://jsonplaceholder.typicode.com/",
                "https://jsonplaceholder.typicode.com",
            ),
            ("http://127.0.0.1:1", "http://127.0.0.1:1"),
            ("http://127.0.0.1:65535/", "http://127.0.0.1:65535"),
            ("http://[::1]:43124", "http://[::1]:43124"),
        ] {
            assert_eq!(endpoint(Some(input)).expect("allowed endpoint"), normalized);
        }
        for denied in [
            "http://localhost:80",
            "http://127.0.0.1",
            "http://127.0.0.1:0",
            "http://127.0.0.1:80/path",
            "http://127.0.0.1:80?query",
            "http://127.0.0.1:80#fragment",
            "http://user@127.0.0.1:80",
            "http://192.0.2.1:80",
            "https://127.0.0.1:443",
            "https://example.com",
        ] {
            assert!(endpoint(Some(denied)).is_err(), "accepted {denied}");
        }
        assert!(endpoint(Some(&"x".repeat(super::MAX_ENDPOINT_BYTES + 1))).is_err());
    }

    #[test]
    fn manifest_documents_machine_readable_utf8_byte_limits() {
        let manifest = JsonPlaceholder::manifest();
        let get_properties = &manifest.capabilities[0].input_schema["properties"];
        assert_eq!(
            get_properties["endpoint"]["x-dekopon-maxUtf8Bytes"],
            super::MAX_ENDPOINT_BYTES
        );
        let create_properties = &manifest.capabilities[1].input_schema["properties"];
        for (field, bytes) in [
            ("title", super::MAX_TITLE_BYTES),
            ("body", super::MAX_BODY_BYTES),
            ("endpoint", super::MAX_ENDPOINT_BYTES),
        ] {
            assert_eq!(create_properties[field]["x-dekopon-maxUtf8Bytes"], bytes);
            assert!(
                create_properties[field]["description"]
                    .as_str()
                    .expect("limit description is text")
                    .contains("UTF-8 bytes")
            );
        }
    }

    #[test]
    fn exact_input_boundaries_are_accepted() {
        for post_id in [1, 100] {
            let output = invoke_with(
                &capability("jsonplaceholder.posts.get"),
                json!({"postId": post_id}),
                |_| {
                    Ok(Response {
                        status: 200,
                        headers: Vec::new(),
                        body: serde_json::to_vec(&json!({
                            "userId": 1,
                            "id": post_id,
                            "title": "t",
                            "body": "b"
                        }))
                        .expect("fixture serializes"),
                    })
                },
            )
            .expect("GET boundary is accepted");
            assert_eq!(output["post"]["id"], post_id);
        }

        for (user_id, title, body) in [
            (1, "t".to_owned(), "b".to_owned()),
            (10, "x".repeat(super::MAX_TITLE_BYTES), "b".to_owned()),
            (1, "é".repeat(super::MAX_TITLE_BYTES / 2), "b".to_owned()),
            (1, "t".to_owned(), "x".repeat(super::MAX_BODY_BYTES)),
            (1, "t".to_owned(), "é".repeat(super::MAX_BODY_BYTES / 2)),
        ] {
            let input_title = title.clone();
            let input_body = body.clone();
            invoke_with(
                &capability("jsonplaceholder.posts.create"),
                json!({"userId": user_id, "title": title, "body": body}),
                move |_| {
                    Ok(Response {
                        status: 201,
                        headers: Vec::new(),
                        body: serde_json::to_vec(&json!({
                            "userId": user_id,
                            "id": 101,
                            "title": input_title,
                            "body": input_body
                        }))
                        .expect("fixture serializes"),
                    })
                },
            )
            .expect("create boundary is accepted");
        }
    }

    #[test]
    fn capability_inputs_are_closed_and_reject_out_of_range_values() {
        for input in [
            json!({"postId": 0}),
            json!({"postId": 101}),
            json!({"postId": 1, "extra": true}),
            json!({"postId": "1"}),
        ] {
            let error = invoke_with(&capability("jsonplaceholder.posts.get"), input, |_| {
                unreachable!("invalid input cannot call HTTP")
            })
            .expect_err("invalid GET input");
            assert_eq!(error.code(), "invalid-input");
        }
        for input in [
            json!({"userId": 0, "title": "t", "body": "b"}),
            json!({"userId": 11, "title": "t", "body": "b"}),
            json!({"userId": 1, "title": "", "body": "b"}),
            json!({"userId": 1, "title": "t", "body": ""}),
            json!({"userId": 1, "title": "t", "body": "b", "extra": true}),
            json!({"userId": 1, "title": "é".repeat(129), "body": "b"}),
            json!({"userId": 1, "title": "t", "body": "é".repeat(2049)}),
        ] {
            let error = invoke_with(&capability("jsonplaceholder.posts.create"), input, |_| {
                unreachable!("invalid input cannot call HTTP")
            })
            .expect_err("invalid create input");
            assert_eq!(error.code(), "invalid-input");
        }
    }

    #[test]
    fn get_status_and_response_matrix_maps_to_stable_errors() {
        let get = |status, body: Vec<u8>| {
            invoke_with(
                &capability("jsonplaceholder.posts.get"),
                json!({"postId": 7}),
                |_| {
                    Ok(Response {
                        status,
                        headers: Vec::new(),
                        body,
                    })
                },
            )
            .expect_err("fixture fails")
        };
        let encoded = |body: Value| serde_json::to_vec(&body).expect("fixture serializes");
        assert_eq!(get(404, encoded(json!({}))).code(), "not-found");
        assert_eq!(get(500, encoded(json!({}))).code(), "unexpected-status");
        assert_eq!(get(200, b"not JSON".to_vec()).code(), "invalid-response");
        for body in [
            json!({"userId": 2, "id": 0, "title": "t", "body": "b"}),
            json!({"userId": 0, "id": 7, "title": "t", "body": "b"}),
            json!({"userId": 11, "id": 7, "title": "t", "body": "b"}),
            json!({"userId": 2, "id": 8, "title": "t", "body": "b"}),
            json!({"userId": 2, "id": 7, "title": "", "body": "b"}),
            json!({"userId": 2, "id": 7, "title": "x".repeat(4097), "body": "b"}),
            json!({"userId": 2, "id": 7, "title": "t", "body": ""}),
            json!({"userId": 2, "id": 7, "title": "t", "body": "x".repeat(16385)}),
        ] {
            assert_eq!(get(200, encoded(body)).code(), "invalid-response");
        }
    }

    #[test]
    fn create_status_echo_and_response_matrix_maps_to_stable_errors() {
        let create = |status, body: Vec<u8>| {
            invoke_with(
                &capability("jsonplaceholder.posts.create"),
                json!({"userId": 3, "title": "title", "body": "body"}),
                |_| {
                    Ok(Response {
                        status,
                        headers: Vec::new(),
                        body,
                    })
                },
            )
            .expect_err("fixture fails")
        };
        let encoded = |body: Value| serde_json::to_vec(&body).expect("fixture serializes");
        for status in [200, 404, 500] {
            assert_eq!(
                create(status, encoded(json!({}))).code(),
                "unexpected-status"
            );
        }
        assert_eq!(create(201, b"not JSON".to_vec()).code(), "invalid-response");
        for body in [
            json!({"userId": 3, "id": 0, "title": "title", "body": "body"}),
            json!({"userId": 0, "id": 101, "title": "title", "body": "body"}),
            json!({"userId": 11, "id": 101, "title": "title", "body": "body"}),
            json!({"userId": 2, "id": 101, "title": "title", "body": "body"}),
            json!({"userId": 3, "id": 101, "title": "different", "body": "body"}),
            json!({"userId": 3, "id": 101, "title": "title", "body": "different"}),
            json!({"userId": 3, "id": 101, "title": "", "body": "body"}),
            json!({"userId": 3, "id": 101, "title": "x".repeat(4097), "body": "body"}),
            json!({"userId": 3, "id": 101, "title": "title", "body": ""}),
            json!({"userId": 3, "id": 101, "title": "title", "body": "x".repeat(16385)}),
        ] {
            assert_eq!(create(201, encoded(body)).code(), "invalid-response");
        }
    }

    #[test]
    fn unknown_capability_fails_before_http() {
        let unknown = invoke_with(
            &capability("jsonplaceholder.posts.delete"),
            json!({}),
            |_| unreachable!("unknown capability cannot call HTTP"),
        )
        .expect_err("unknown capability fails");
        assert_eq!(unknown.code(), "unknown-capability");
    }
}
