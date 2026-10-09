//! Bounded typed JSONPlaceholder capabilities over broker-mediated HTTP.

#[cfg(test)]
use dekopon_provider_sdk::CapabilityId;
use dekopon_provider_sdk::provider::endpoint::Base;
use dekopon_provider_sdk::provider::{
    self, Capability, Code, Failure, Header, Http, HttpError, Proposal, Provider, Request,
    Response, Settings, Stdout, Usage,
};
use dekopon_provider_sdk::schemars::JsonSchema;
use dekopon_provider_sdk::{EffectKind, RiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;
use std::io::{Read, Write};

mod commands;

/// Gets one post by numeric ID.
#[cfg(test)]
pub(crate) const POSTS_GET: &str = "jsonplaceholder.posts.get";
/// Creates one non-persistent post.
#[cfg(test)]
pub(crate) const POSTS_CREATE: &str = "jsonplaceholder.posts.create";
/// The command word this provider contributes to the sandboxed shell.
///
/// Separator-free on purpose: `dekopon-core` refuses a command word that parses as a capability
/// identifier. The tree below it mirrors the capability identifiers, so `placeholder posts get` is
/// `jsonplaceholder.posts.get`.
pub(crate) const COMMAND_WORD: &str = "placeholder";

const JSONPLACEHOLDER_API: Base = Base::from_static("https://jsonplaceholder.typicode.com");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(missing_docs)]
pub struct JsonPlaceholderSettings {
    #[serde(default = "default_base")]
    base_url: Base,
}

fn default_base() -> Base {
    JSONPLACEHOLDER_API
}

const MAX_TITLE_BYTES: usize = 256;
const MAX_BODY_BYTES: usize = 4 * 1024;
const MAX_RESPONSE_TITLE_BYTES: usize = 4 * 1024;
const MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024;

/// A narrow broker-mediated JSONPlaceholder provider.
pub struct JsonPlaceholder;
/// One bounded post read.
pub struct GetPost;
/// One explicitly authorized external write.
pub struct CreatePost;

impl Provider for JsonPlaceholder {
    const ID: &'static str = "jsonplaceholder";
    const COMMAND_WORDS: &'static [&'static str] = &[COMMAND_WORD];
    const DESCRIPTION: &'static str =
        "Reads and creates bounded JSONPlaceholder posts through broker HTTP";
    type Args = commands::Placeholder;
    type Capabilities = (GetPost, CreatePost);
    fn propose(args: Self::Args, stdin_piped: bool) -> Result<Proposal<Self>, Usage> {
        commands::propose(args, stdin_piped)
    }
}

/// Stable sanitized guest failure.
#[derive(Debug)]
pub struct ProviderError {
    code: &'static str,
    message: &'static str,
}
impl ProviderError {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
    #[cfg(test)]
    fn code(&self) -> &str {
        self.code
    }
    #[cfg(test)]
    fn message(&self) -> &str {
        self.message
    }
}
impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}
impl Failure for ProviderError {
    fn code(&self) -> Code {
        match self.code {
            "invalid-input" => Code::INVALID_INPUT,
            "invalid-request" => Code::new("invalid-request"),
            "http-failed" => Code::new("http-failed"),
            "not-found" => Code::new("not-found"),
            "unexpected-status" => Code::new("unexpected-status"),
            "invalid-response" => Code::new("invalid-response"),
            "output-closed" => Code::new("output-closed"),
            "usage" => Code::USAGE,
            _ => Code::UNKNOWN_CAPABILITY,
        }
    }
}

fn emit(value: Value, out: &mut Stdout) -> Result<(), ProviderError> {
    serde_json::to_writer(&mut *out, &value)
        .map_err(|_| ProviderError::new("output-closed", "stdout's reader has gone"))?;
    out.write_all(b"\n")
        .map_err(|_| ProviderError::new("output-closed", "stdout's reader has gone"))
}

impl Capability for GetPost {
    type Provider = JsonPlaceholder;
    const NAME: &'static str = "posts.get";
    const DESCRIPTION: &'static str = "Gets one JSONPlaceholder post by numeric ID";
    const EFFECT: EffectKind = EffectKind::ReadOnly;
    const RISK: RiskLevel = RiskLevel::Low;
    type Input = GetPostInput;
    type Needs = (Settings<JsonPlaceholderSettings>, Http);
    type Error = ProviderError;
    fn run(
        input: Self::Input,
        (settings, http): Self::Needs,
        out: &mut Stdout,
    ) -> Result<(), Self::Error> {
        emit(
            get_post(input, &settings.into_inner().base_url, |request| {
                http.send(request)
            })?,
            out,
        )
    }
}
impl Capability for CreatePost {
    type Provider = JsonPlaceholder;
    const NAME: &'static str = "posts.create";
    const DESCRIPTION: &'static str = "Creates one non-persistent JSONPlaceholder post";
    const EFFECT: EffectKind = EffectKind::ExternalWrite;
    const RISK: RiskLevel = RiskLevel::Medium;
    type Input = CreatePostInput;
    type Needs = (Settings<JsonPlaceholderSettings>, Http);
    type Error = ProviderError;
    fn run(
        input: Self::Input,
        (settings, http): Self::Needs,
        out: &mut Stdout,
    ) -> Result<(), Self::Error> {
        emit(
            create_post(input, &settings.into_inner().base_url, |request| {
                http.send(request)
            })?,
            out,
        )
    }
}

#[cfg(test)]
fn invoke_with<F>(capability: &CapabilityId, input: Value, send: F) -> Result<Value, ProviderError>
where
    F: FnOnce(Request) -> Result<Response, HttpError>,
{
    match capability.as_str() {
        POSTS_GET => get_post(
            serde_json::from_value(input).map_err(|_| invalid_input())?,
            &default_base(),
            send,
        ),
        POSTS_CREATE => create_post(
            serde_json::from_value(input).map_err(|_| invalid_input())?,
            &default_base(),
            send,
        ),
        _ => Err(ProviderError::new(
            "unknown-capability",
            "unsupported JSONPlaceholder capability",
        )),
    }
}

/// Closed input for a single bounded GET.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GetPostInput {
    #[schemars(range(min = 1, max = 100))]
    post_id: u32,
}

/// Closed input for a synthetic external write.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreatePostInput {
    #[schemars(range(min = 1, max = 10))]
    user_id: u32,
    /// Post title, limited to 256 UTF-8 bytes.
    #[schemars(length(min = 1, max = 256), extend("x-dekopon-maxUtf8Bytes" = 256))]
    title: String,
    /// Post body, limited to 4096 UTF-8 bytes.
    #[schemars(length(min = 1, max = 4096), extend("x-dekopon-maxUtf8Bytes" = 4096))]
    body: String,
    /// Marker only: the piped body is read after authorization, never included in a proposal.
    #[serde(default, skip_serializing_if = "is_false")]
    stdin_piped: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Post {
    user_id: u32,
    id: u32,
    title: String,
    body: String,
}

fn get_post<F>(input: GetPostInput, base: &Base, send: F) -> Result<Value, ProviderError>
where
    F: FnOnce(Request) -> Result<Response, HttpError>,
{
    if !(1..=100).contains(&input.post_id) {
        return Err(invalid_input());
    }
    let request = Request::new(
        "GET",
        base.join(&format!("/posts/{}", input.post_id))
            .map_err(|_| invalid_request())?,
    )
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

fn create_post<F>(input: CreatePostInput, base: &Base, send: F) -> Result<Value, ProviderError>
where
    F: FnOnce(Request) -> Result<Response, HttpError>,
{
    let mut input = input;
    if input.stdin_piped {
        if input.body != "-" {
            return Err(invalid_input());
        }
        let stdin = provider::stdin().ok_or_else(|| {
            ProviderError::new(
                "usage",
                "placeholder posts create --body -: nothing was piped in",
            )
        })?;
        let mut bytes = Vec::new();
        stdin
            .take((MAX_BODY_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid_input())?;
        input.body = String::from_utf8(bytes).map_err(|_| invalid_input())?;
    }
    validate_create_input(&input)?;
    let body = serde_json::to_vec(&json!({
        "userId": input.user_id,
        "title": &input.title,
        "body": &input.body
    }))
    .map_err(|_| invalid_request())?;
    let request = Request::new("POST", base.join("/posts").map_err(|_| invalid_request())?)
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

fn json_header(name: &'static str) -> Result<Header, ProviderError> {
    Header::text(name, "application/json").map_err(|_| invalid_request())
}

fn invalid_input() -> ProviderError {
    ProviderError::new(
        "invalid-input",
        "input does not match the capability contract",
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

#[cfg(target_arch = "wasm32")]
mod guest {
    dekopon_provider_sdk::export!(super::JsonPlaceholder);
}

#[cfg(test)]
mod tests {
    use dekopon_provider_sdk::provider::{self, Header, HttpErrorCode, Response};
    use dekopon_provider_sdk::{EffectKind, RiskLevel};
    use serde_json::{Value, json};

    use super::{COMMAND_WORD, JsonPlaceholder, invoke_with};

    fn capability(value: &str) -> dekopon_provider_sdk::CapabilityId {
        value.parse().expect("valid capability fixture")
    }

    #[test]
    fn manifest_separates_read_and_external_write_authority() {
        let manifest = provider::manifest::<JsonPlaceholder>().expect("manifest");
        assert_eq!(manifest.id.as_str(), "jsonplaceholder");
        assert_eq!(manifest.command_words, [COMMAND_WORD]);
        assert_eq!(manifest.capabilities.len(), 2);
        assert_eq!(manifest.capabilities[0].effect, EffectKind::ReadOnly);
        assert_eq!(manifest.capabilities[0].risk, RiskLevel::Low);
        assert_eq!(manifest.capabilities[1].effect, EffectKind::ExternalWrite);
        assert_eq!(manifest.capabilities[1].risk, RiskLevel::Medium);
    }

    #[test]
    fn get_uses_only_the_bounded_post_path_and_parses_mock_response() {
        let output = invoke_with(
            &capability("jsonplaceholder.posts.get"),
            json!({"postId": 7}),
            |request| {
                assert_eq!(request.method, "GET");
                assert_eq!(request.uri, "https://jsonplaceholder.typicode.com/posts/7");
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
                "body": "created body"
            }),
            |request| {
                assert_eq!(request.method, "POST");
                assert_eq!(request.uri, "https://jsonplaceholder.typicode.com/posts");
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
    fn invalid_id_fails_closed() {
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
                Err(dekopon_provider_sdk::provider::HttpError {
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
    fn manifest_documents_machine_readable_utf8_byte_limits() {
        let manifest = provider::manifest::<JsonPlaceholder>().expect("manifest");
        for capability in &manifest.capabilities {
            assert_eq!(capability.input_schema["additionalProperties"], false);
            let properties = capability.input_schema["properties"].as_object().unwrap();
            assert!(!properties.keys().any(|key| key.to_ascii_lowercase().contains("url") || key.contains("endpoint")));
        }
        let create_properties = &manifest.capabilities[1].input_schema["properties"];
        for (field, bytes) in [
            ("title", super::MAX_TITLE_BYTES),
            ("body", super::MAX_BODY_BYTES),
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
