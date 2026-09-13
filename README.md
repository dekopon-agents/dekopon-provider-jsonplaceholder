# JSONPlaceholder provider for Dekopon

A narrow WebAssembly component that reads and creates bounded JSONPlaceholder posts through the broker-owned `dekopon:http/client@1.0.0` interface.

## Authority and behavior

The provider preserves two separate typed capabilities:

- `jsonplaceholder.posts.get`: low-risk `read-only` GET of post IDs 1–100.
- `jsonplaceholder.posts.create`: medium-risk `external-write` POST. JSONPlaceholder returns a synthetic record and does **not** persist it, but the call remains an external effect and may have executed even if response validation fails.

The guest has no transport, credentials, WASI, filesystem, sockets, environment, subprocess, or generic request escape hatch. It imports exactly the buffered HTTP client. It accepts only `https://jsonplaceholder.typicode.com` (with an optional trailing slash) or, for deterministic tests, an explicit literal loopback `http://IP:PORT` endpoint. Broker constraints must independently grant the exact effective authority and method.

GET sends `/posts/{postId}` with `Accept: application/json`. Create sends `/posts` with `Accept` and `Content-Type: application/json` and a body containing only `userId`, `title`, and `body`. Input and response byte limits, status mappings, echoed create fields, and transport-error redaction are enforced in the guest.

## Broker configuration

Read and write require independent constraint and Cedar entries; read authority never implies write authority.

```yaml
constraintSets:
  jsonplaceholder.posts.get:
    provider: jsonplaceholder
    effect: read-only
    risk: Low
    constraints:
      timeoutMs: 5000
      maxOutputBytes: 65536
      http:
        allowedHosts: [jsonplaceholder.typicode.com]
        allowedMethods: [GET]
        maxRequests: 1
        maxRequestBytes: 8192
        maxResponseBytes: 65536
        allowPlaintextLoopback: false
  jsonplaceholder.posts.create:
    provider: jsonplaceholder
    effect: external-write
    risk: Medium
    constraints:
      timeoutMs: 5000
      maxOutputBytes: 65536
      http:
        allowedHosts: [jsonplaceholder.typicode.com]
        allowedMethods: [POST]
        maxRequests: 1
        maxRequestBytes: 8192
        maxResponseBytes: 65536
        allowPlaintextLoopback: false
```

The provider needs no secrets. Paths, queries, headers, bodies, transport errors, inputs, and outputs must not be copied into audit records; the broker may record sanitized method, authority, status, and byte-count metadata.

## Build and inspect

The checked-in WIT is caller-owned and byte-compared to the exact `dekopon-provider-sdk = 0.13.0` and `dekopon-provider-http = 0.13.0` crate contracts. The deterministic build requires Rust 1.98.1 (`rustc 1.98.1 (48a229cea 2026-09-01)`) and `wasm-tools 1.259.0`.

```console
./build.sh
./scripts/verify-component.sh
```

The ignored output is `jsonplaceholder-provider.wasm` plus its `.sha256`. The decoded component exports the provider surface, imports exactly `dekopon:http/client@1.0.0`, and imports no WASI. An empty Wasmtime linker intentionally rejects it; execution requires the broker.

## Acceptance

Tests use injected native responses or literal loopback listeners only. They never contact the public JSONPlaceholder service.

```console
cargo +1.98.1 fmt --all -- --check
cargo +1.98.1 clippy --locked --all-targets -- -D warnings
cargo +1.98.1 test --locked --lib
cargo +1.98.1 check --locked --all-targets
cargo +1.98.1 check --locked --target wasm32-unknown-unknown
cargo deny --locked check advisories licenses bans sources
./build.sh
./scripts/verify-component.sh
cargo +1.98.1 test --locked --test broker_host -- --nocapture
./scripts/test-direct-refusal.sh
./scripts/check-reproducible.sh
```

CI additionally checks generated dependency/license inventories, deterministic CycloneDX SBOMs, shell scripts, YAML, full-SHA Action pins, the exact two-file release layout, two clean archive builds, and absence of tracked Wasm.

## Release

Each version is released only by the tag workflow after explicit human authorization. See [RELEASE.md](RELEASE.md). Actions creates exactly `jsonplaceholder-provider.wasm` and `jsonplaceholder-provider.wasm.sha256`, provenance and CycloneDX attestations, a non-latest GitHub release, and one `ghcr.io/dekopon-agents/provider-jsonplaceholder:<version>` OCI manifest with one `application/wasm` layer. Do not create a remote, tag, release, or package by hand.

Licensed under MIT OR Apache-2.0. See the license files and `THIRD_PARTY_NOTICES.md`.
