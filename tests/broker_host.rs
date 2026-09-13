//! Broker-mediated component acceptance against literal loopback listeners only.
//! No test resolves or contacts a public hostname.

use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Duration,
};

use dekopon_broker_host::{BrokerHostError, BrokerHostLimits, BrokerProviderRegistry};
use dekopon_capability::{
    AuthorizedInvocation, ExecutionConstraints, HttpConstraints, ProposedInvocation,
    broker::AuthorizationGate,
};
use dekopon_core::{Actor, AgentId, CapabilityId, InvocationId, PrincipalId, ProviderId, TraceId};
use dekopon_provider_sdk::CommandRunOutcome;
use serde_json::{Value, json};

/// A W3C trace identifier: 16 non-zero bytes rendered as 32 lowercase hex digits.
const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

fn component() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("jsonplaceholder-provider.wasm")
}

fn authorized(
    id: &str,
    capability: &str,
    input: Value,
    constraints: ExecutionConstraints,
) -> AuthorizedInvocation {
    let proposal = ProposedInvocation::new(
        id.parse::<InvocationId>().expect("valid invocation ID"),
        capability
            .parse::<CapabilityId>()
            .expect("valid capability"),
        Actor::Agent {
            agent: "jsonplaceholder-test"
                .parse::<AgentId>()
                .expect("valid agent"),
        },
        TRACE_ID.parse::<TraceId>().expect("valid trace"),
        input,
    );
    AuthorizationGate::new()
        .authorize(
            proposal,
            "jsonplaceholder"
                .parse::<ProviderId>()
                .expect("valid provider"),
            format!("decision-{id}"),
            "broker-test"
                .parse::<PrincipalId>()
                .expect("valid principal"),
            "policy-test".to_owned(),
            constraints,
        )
        .expect("bounded fixture authorization")
}

fn profile(authority: &str, method: &str) -> ExecutionConstraints {
    ExecutionConstraints {
        timeout_ms: 5_000,
        max_output_bytes: 65_536,
        http: Some(HttpConstraints {
            allowed_hosts: vec![authority.to_owned()],
            allowed_methods: vec![method.to_owned()],
            max_requests: 1,
            max_request_bytes: 8_192,
            max_response_bytes: 65_536,
            // Plaintext is test-only and independently restricted to the exact loopback socket.
            allow_plaintext_loopback: true,
        }),
        storage: None,
        secret_use: None,
    }
}

fn mock_http(response: Vec<u8>) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("fixture address");
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fixture request");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set read timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    request.extend_from_slice(&buffer[..read]);
                    if let Some(headers_end) = request
                        .windows(4)
                        .position(|window| window == b"\r\n\r\n")
                        .map(|offset| offset + 4)
                    {
                        let headers = String::from_utf8_lossy(&request[..headers_end]);
                        let content_length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        if request.len() >= headers_end + content_length {
                            break;
                        }
                    }
                }
                Err(error) => panic!("read request: {error}"),
            }
        }
        sender.send(request).expect("record request");
        stream.write_all(&response).expect("write response");
        stream.flush().expect("flush response");
    });
    (format!("127.0.0.1:{}", address.port()), receiver, handle)
}

fn response(status: &str, body: Value) -> Vec<u8> {
    let body = serde_json::to_vec(&body).expect("fixture serializes");
    let mut wire = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    wire.extend(body);
    wire
}

#[tokio::test(flavor = "multi_thread")]
async fn broker_loads_exact_separated_manifest_with_the_placeholder_command_word() {
    let registry = BrokerProviderRegistry::load([component()], BrokerHostLimits::default())
        .await
        .expect("broker linker loads HTTP provider");
    assert_eq!(registry.command_words(), ["placeholder"]);
    let manifest = registry.manifests().next().expect("one manifest");
    assert_eq!(manifest.id.as_str(), "jsonplaceholder");
    assert_eq!(manifest.capabilities.len(), 2);
    assert_eq!(
        manifest.capabilities[0].id.as_str(),
        "jsonplaceholder.posts.get"
    );
    assert_eq!(
        manifest.capabilities[1].id.as_str(),
        "jsonplaceholder.posts.create"
    );
}

/// The shipped component's `run-command` export, through the broker host: a well-formed argv is a
/// proposal and nothing more, and a usage error is rendered text that authorizes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn placeholder_word_proposes_or_renders_through_the_run_command_export() {
    let registry = BrokerProviderRegistry::load([component()], BrokerHostLimits::default())
        .await
        .expect("broker linker loads HTTP provider");
    let argv = |words: &[&str]| {
        words
            .iter()
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>()
    };

    let outcome = registry
        .run_command(
            "placeholder",
            &argv(&[
                "posts",
                "create",
                "--user-id",
                "3",
                "--title",
                "t",
                "--body",
                "-",
            ]),
            Some("piped body"),
        )
        .await
        .expect("run-command answers");
    let CommandRunOutcome::Proposed {
        capability, input, ..
    } = outcome
    else {
        panic!("expected a proposal, got {outcome:?}");
    };
    assert_eq!(capability.as_str(), "jsonplaceholder.posts.create");
    assert_eq!(
        input,
        json!({"userId": 3, "title": "t", "body": "piped body"})
    );

    let outcome = registry
        .run_command("placeholder", &argv(&["posts", "get"]), None)
        .await
        .expect("run-command answers");
    let CommandRunOutcome::Rendered {
        stdout,
        stderr,
        status,
    } = outcome
    else {
        panic!("expected rendered usage, got {outcome:?}");
    };
    assert_eq!(status, 2);
    assert!(stdout.is_empty(), "{stdout}");
    assert!(
        stderr.contains("Usage: placeholder posts get --post-id <ID>"),
        "{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn exact_get_grant_executes_one_bounded_request_and_records_authority() {
    let (authority, received, server) = mock_http(response(
        "200 OK",
        json!({"userId": 2, "id": 7, "title": "mock", "body": "body"}),
    ));
    let registry = BrokerProviderRegistry::load([component()], BrokerHostLimits::default())
        .await
        .expect("broker loads component");
    let output = registry
        .invoke(
            authorized(
                "get-success",
                "jsonplaceholder.posts.get",
                json!({"postId": 7, "endpoint": format!("http://{authority}")}),
                profile(&authority, "GET"),
            ),
            None,
        )
        .await
        .expect("exact read grant executes");
    assert_eq!(output.output["post"]["id"], 7);
    assert_eq!(output.http_calls.len(), 1);
    assert_eq!(output.http_calls[0].authority, authority);
    assert_eq!(output.http_calls[0].method, "GET");
    assert_eq!(output.http_calls[0].status, Some(200));
    assert!(!output.http_calls[0].credential_injected);
    let wire = received.recv().expect("request recorded");
    assert!(wire.starts_with(b"GET /posts/7 HTTP/1.1\r\n"));
    assert!(
        !String::from_utf8_lossy(&wire)
            .to_ascii_lowercase()
            .contains("authorization:")
    );
    server.join().expect("fixture exits");
}

#[tokio::test(flavor = "multi_thread")]
async fn create_requires_an_independent_post_grant_and_sends_exact_json() {
    let (authority, received, server) = mock_http(response(
        "201 Created",
        json!({"userId": 3, "id": 101, "title": "created", "body": "payload"}),
    ));
    let registry = BrokerProviderRegistry::load([component()], BrokerHostLimits::default())
        .await
        .expect("broker loads component");

    let denied = registry
        .invoke(
            authorized(
                "post-denied-by-read",
                "jsonplaceholder.posts.create",
                json!({
                    "userId": 3, "title": "created", "body": "payload",
                    "endpoint": format!("http://{authority}")
                }),
                profile(&authority, "GET"),
            ),
            None,
        )
        .await
        .expect_err("read grant never implies write");
    assert!(matches!(
        denied.error.as_ref(),
        BrokerHostError::HostCallRejected {
            reason: "denied",
            ..
        }
    ));
    assert!(denied.http_calls.is_empty());

    let output = registry
        .invoke(
            authorized(
                "post-success",
                "jsonplaceholder.posts.create",
                json!({
                    "userId": 3, "title": "created", "body": "payload",
                    "endpoint": format!("http://{authority}")
                }),
                profile(&authority, "POST"),
            ),
            None,
        )
        .await
        .expect("write grant executes");
    assert_eq!(output.output["post"]["id"], 101);
    assert_eq!(output.http_calls[0].method, "POST");
    assert_eq!(output.http_calls[0].authority, authority);
    let wire = received.recv().expect("request recorded");
    assert!(wire.starts_with(b"POST /posts HTTP/1.1\r\n"));
    let body = wire
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|offset| &wire[offset + 4..])
        .expect("request has headers");
    assert_eq!(
        serde_json::from_slice::<Value>(body).expect("JSON request"),
        json!({"userId": 3, "title": "created", "body": "payload"})
    );
    server.join().expect("fixture exits");
}

#[tokio::test(flavor = "multi_thread")]
async fn post_effect_response_failure_is_reported_as_potentially_executed() {
    let (authority, received, server) = mock_http(response(
        "201 Created",
        json!({"userId": 9, "id": 101, "title": "wrong", "body": "wrong"}),
    ));
    let registry = BrokerProviderRegistry::load([component()], BrokerHostLimits::default())
        .await
        .expect("broker loads component");
    let failure = registry
        .invoke(
            authorized(
                "post-invalid-response",
                "jsonplaceholder.posts.create",
                json!({
                    "userId": 3, "title": "created", "body": "payload",
                    "endpoint": format!("http://{authority}")
                }),
                profile(&authority, "POST"),
            ),
            None,
        )
        .await
        .expect_err("invalid post-effect response fails");
    assert!(matches!(
        failure.error.as_ref(),
        BrokerHostError::ProviderFailure { code, .. } if code == "invalid-response"
    ));
    assert_eq!(failure.http_calls.len(), 1);
    assert_eq!(failure.http_calls[0].status, Some(201));
    assert!(
        received
            .recv()
            .expect("request executed")
            .starts_with(b"POST /posts")
    );
    server.join().expect("fixture exits");
}

#[tokio::test(flavor = "multi_thread")]
async fn bounded_response_runs_under_committed_fuel_and_memory_ceilings() {
    const FUEL: u64 = 64_000_000;
    const MEMORY: usize = 16 * 1024 * 1024;
    let (authority, _received, server) = mock_http(response(
        "200 OK",
        json!({
            "userId": 10,
            "id": 100,
            "title": "t".repeat(4 * 1024),
            "body": "b".repeat(16 * 1024)
        }),
    ));
    let registry = BrokerProviderRegistry::load(
        [component()],
        BrokerHostLimits {
            fuel: FUEL,
            max_memory_bytes: MEMORY,
            ..BrokerHostLimits::default()
        },
    )
    .await
    .expect("component describes under fixed resources");
    let output = registry
        .invoke(
            authorized(
                "bounded-response",
                "jsonplaceholder.posts.get",
                json!({"postId": 100, "endpoint": format!("http://{authority}")}),
                profile(&authority, "GET"),
            ),
            None,
        )
        .await
        .expect("maximum valid post fits fixed resources");
    assert_eq!(output.output["post"]["id"], 100);
    assert_eq!(output.http_calls.len(), 1);
    server.join().expect("fixture exits");
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_authority_and_plaintext_policy_fail_before_network() {
    let registry = BrokerProviderRegistry::load([component()], BrokerHostLimits::default())
        .await
        .expect("broker loads component");
    for (id, constraints) in [
        ("wrong-authority", profile("127.0.0.1:10", "GET")),
        ("wrong-method", profile("127.0.0.1:9", "POST")),
        ("missing-http", ExecutionConstraints::default()),
    ] {
        let failure = registry
            .invoke(
                authorized(
                    id,
                    "jsonplaceholder.posts.get",
                    json!({"postId": 1, "endpoint": "http://127.0.0.1:9"}),
                    constraints,
                ),
                None,
            )
            .await
            .expect_err("grant mismatch fails closed");
        assert!(matches!(
            failure.error.as_ref(),
            BrokerHostError::HostCallRejected { .. }
        ));
        assert!(failure.http_calls.is_empty());
    }
}
