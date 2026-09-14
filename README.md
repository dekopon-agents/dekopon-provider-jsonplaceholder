# JSONPlaceholder provider for Dekopon

A narrow WebAssembly component that reads and creates bounded JSONPlaceholder posts through the broker-owned `dekopon:http/client@1.0.0` interface.

## Authority and behavior

The provider preserves two separate typed capabilities:

- `jsonplaceholder.posts.get`: low-risk `read-only` GET of post IDs 1–100.
- `jsonplaceholder.posts.create`: medium-risk `external-write` POST. JSONPlaceholder returns a synthetic record and does **not** persist it, but the call remains an external effect and may have executed even if response validation fails.

The guest has no transport, credentials, WASI, filesystem, sockets, environment, subprocess, or generic request escape hatch. It imports exactly the buffered HTTP client. It accepts only `https://jsonplaceholder.typicode.com` (with an optional trailing slash) or, for deterministic tests, an explicit literal loopback `http://IP:PORT` endpoint. Broker constraints must independently grant the exact effective authority and method.

GET sends `/posts/{postId}` with `Accept: application/json`. Create sends `/posts` with `Accept` and `Content-Type: application/json` and a body containing only `userId`, `title`, and `body`. Input and response byte limits, status mappings, echoed create fields, and transport-error redaction are enforced in the guest.

## The `placeholder` command word

```
placeholder posts get --post-id 7
placeholder posts create --user-id 1 --title "hello" --body "first post"
placeholder posts create --user-id 1 --title "hello" --body -   # reads the value piped into the word
placeholder --help                                               # rendered by the guest, at exit 0
```

The tree is the capability ID with the provider prefix swapped for the word: `placeholder posts get` proposes `jsonplaceholder.posts.get`. Each flag is the kebab-case of the wire field it fills, so `--post-id` is `postId`. `--help`, `--version`, and every usage error are rendered inside the component and authorize nothing. A well-formed argv becomes a *proposal* carrying exactly the input a direct invocation sends, and it travels the same constraint-set lookup and Cedar. ID ranges, byte limits, and the endpoint allowlist are checked once, by the same input parser, so `--post-id 0` parses and is then refused as `invalid-input` before any HTTP.

`--endpoint` is on both verbs because it is a wire field; it exists for loopback tests, and production constraint sets allow only `jsonplaceholder.typicode.com` whatever the flag says.

```console
$ placeholder --help
Read and create JSONPlaceholder posts

Usage: placeholder <COMMAND>

Commands:
  posts  Read and create posts
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

$ placeholder posts get --help
Get one post by ID

Usage: placeholder posts get [OPTIONS] --post-id <ID>

Options:
      --post-id <ID>    The post to read, 1 to 100
      --endpoint <URL>  Production JSONPlaceholder HTTPS (the default) or a literal loopback http://IP:PORT
  -h, --help            Print help

$ placeholder posts create --help
Create one post; JSONPlaceholder echoes it back and does not persist it

Usage: placeholder posts create [OPTIONS] --user-id <ID> --title <TEXT> --body <TEXT>

Options:
      --user-id <ID>    The author, 1 to 10
      --title <TEXT>    The title, up to 256 UTF-8 bytes
      --body <TEXT>     The body, up to 4096 UTF-8 bytes. `-` reads the piped value
      --endpoint <URL>  Production JSONPlaceholder HTTPS (the default) or a literal loopback http://IP:PORT
  -h, --help            Print help
```

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

The checked-in WIT is caller-owned and byte-compared to the exact `dekopon-provider-sdk = 0.15.0` and `dekopon-provider-http = 0.15.0` crate contracts. The deterministic build requires Rust 1.98.1 (`rustc 1.98.1 (48a229cea 2026-09-01)`) and `wasm-tools 1.259.0`.

```console
../provider-workflows/build.sh
```

The output is `jsonplaceholder-provider.wasm` plus its `.sha256`. The decoded component exports exactly `describe`, `invoke`, and `run-command` (the `dekopon:provider/provider-cli@0.3.0` world), imports exactly `dekopon:http/client@1.0.0`, and imports no WASI. An empty Wasmtime linker intentionally rejects it; execution requires the broker.

## Acceptance

Tests use injected native responses or literal loopback listeners only. They never contact the public JSONPlaceholder service.

```console
cargo +1.98.1 fmt --all --check
cargo +1.98.1 clippy --locked --workspace --all-targets -- -D warnings
cargo deny --all-features check bans licenses sources advisories
../provider-workflows/build.sh
DEKOPON_PROVIDER_COMPONENT=$PWD/jsonplaceholder-provider.wasm cargo +1.98.1 test --locked --workspace
```

`DEKOPON_PROVIDER_COMPONENT` must point at the built component; the broker-host tests panic without it. The shared `ci / validate` workflow in `dekopon-agents/provider-workflows` runs the same gates, plus the reproducible build check, the CycloneDX SBOM, and the release asset layout.

## Release

Each version is released only by the tag workflow after explicit human authorization. See [RELEASE.md](RELEASE.md). Actions creates exactly `jsonplaceholder-provider.wasm` and `jsonplaceholder-provider.wasm.sha256`, provenance and CycloneDX attestations, a non-latest GitHub release, and one `ghcr.io/dekopon-agents/provider-jsonplaceholder:<version>` OCI manifest with one `application/wasm` layer. Do not create a remote, tag, release, or package by hand.

Licensed under MIT OR Apache-2.0. See the license files. The SBOM asset generated by the release workflow is the third-party disclosure; there is no `THIRD_PARTY_NOTICES.md` in the repository.
