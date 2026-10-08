//! GET-only broker policy must reject create before any piped bytes or HTTP reach the guest.
use dekopon_broker::{
    AuthenticatedContext, Broker, BrokerLimits, CapabilityRoute, ConstraintCatalog, ConstraintSet,
    CredentialStore, IdentityDirectory, InMemoryAuditLog, InvocationRequest, PolicyEngine,
    PolicyWorld,
};
use dekopon_broker_host::{BrokerHostLimits, BrokerProviderRegistry, asset::AssetInputs};
use dekopon_broker_protocol::TraceParent;
use dekopon_capability::{EffectKind, ExecutionConstraints, HttpConstraints, InvocationOutcome};
use dekopon_core::{Actor, AgentId, CapabilityId, PrincipalId, ProviderId, RiskLevel};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

fn principal(id: &str) -> PrincipalId {
    id.parse().expect("fixed principal")
}
fn capability(id: &str) -> CapabilityId {
    id.parse().expect("fixed capability")
}
fn request(id: &str, capability_id: &str, input: serde_json::Value) -> InvocationRequest {
    InvocationRequest {
        id: id.parse().expect("fixed invocation"),
        capability: capability(capability_id),
        trace_parent: TraceParent::new([7; 16], [3; 8], 1).expect("trace"),
        secret_use: None,
        input,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn read_only_catalog_and_cedar_deny_create_before_http() {
    let component =
        PathBuf::from(std::env::var_os("DEKOPON_PROVIDER_COMPONENT").expect("built component"));
    let registry = BrokerProviderRegistry::load([component], BrokerHostLimits::default())
        .await
        .expect("load component");
    let get = capability("jsonplaceholder.posts.get");
    let create = capability("jsonplaceholder.posts.create");
    let provider: ProviderId = "jsonplaceholder".parse().expect("provider");
    let world = PolicyWorld::new(
        [principal("allowed-caller")],
        [
            (get.clone(), provider.clone()),
            (create.clone(), provider.clone()),
        ],
    )
    .expect("policy world");
    let policy = r#"@id("placeholder-get-only") permit(
        principal == Dekopon::Principal::"allowed-caller",
        action == Dekopon::Action::"jsonplaceholder.posts.get",
        resource == Dekopon::Provider::"jsonplaceholder"
    ) when { context.agent == "placeholder-test" && context.via == "gateway" };"#;
    let constraints = ConstraintSet {
        route: CapabilityRoute::Generic,
        provider,
        effect: EffectKind::ReadOnly,
        risk: RiskLevel::Low,
        credential: None,
        constraints: ExecutionConstraints {
            timeout_ms: 5_000,
            http: Some(HttpConstraints {
                allowed_hosts: vec!["127.0.0.1:9".into()],
                allowed_methods: vec!["GET".into()],
                max_requests: 1,
                max_request_bytes: 8_192,
                max_response_bytes: 65_536,
                allow_plaintext_loopback: true,
                propagate_trace: false,
            }),
            storage: None,
            asset: None,
            secret_use: None,
        },
    };
    let audit = Arc::new(InMemoryAuditLog::new(16).expect("audit"));
    let broker = Broker::new(
        registry,
        principal("broker-test"),
        "policy-test".into(),
        PolicyEngine::new(policy, &world).expect("Cedar policy"),
        ConstraintCatalog::new([(get, constraints)]).expect("GET-only catalog"),
        CredentialStore::empty(),
        IdentityDirectory::empty(),
        Arc::clone(&audit),
        BrokerLimits::default(),
    )
    .expect("broker accepts an ungranted advertised write");
    let context = AuthenticatedContext::attested(
        principal("allowed-caller"),
        Actor::Agent {
            agent: "placeholder-test".parse::<AgentId>().expect("agent"),
        },
        principal("gateway"),
        "slack.t0123abc.u9xyz".parse().expect("route"),
    )
    .expect("attested context");
    // No server exists at :9. If policy wrongly grants POST, the result differs from Denied.
    let input = json!({"userId": 3, "title": "t", "body": "-", "stdinPiped": true});
    let denied = broker
        .invoke(
            &context,
            None,
            None,
            request("create-denied", create.as_str(), input),
            AssetInputs::default(),
        )
        .await
        .expect("policy denial is an outcome");
    assert_eq!(denied.result.outcome, InvocationOutcome::Denied);
    assert!(
        matches!(
            denied.result.error.as_deref(),
            Some("unconstrained-capability")
        ),
        "denial: {:?}",
        denied.result.error
    );
    assert_eq!(denied.result.exit_status, None);
    let records = serde_json::to_string(&audit.records()).expect("audit JSON");
    assert!(!records.contains("private-piped-canary"));
}
