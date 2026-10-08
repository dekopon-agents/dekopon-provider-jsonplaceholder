# JSONPlaceholder provider for Dekopon

A narrow WebAssembly component that reads and creates bounded JSONPlaceholder posts through the broker-owned `dekopon:http/client@1.1.0` interface.

## Authority and behavior

The provider preserves two separate typed capabilities:

- `jsonplaceholder.posts.get`: low-risk `read-only` GET of post IDs 1–100.
- `jsonplaceholder.posts.create`: medium-risk `external-write` POST. JSONPlaceholder returns a synthetic record and does **not** persist it, but the call remains an external effect and may have executed even if response validation fails.

The guest has no transport, credentials, WASI, filesystem, sockets, environment, subprocess, or generic request escape hatch. It imports exactly the broker HTTP client and stdio streams (`dekopon:http/client@1.1.0`, `dekopon:stdio/streams@0.1.0`), and exports `dekopon:provider/provider@0.4.0`. It accepts only `https://jsonplaceholder.typicode.com` (with an optional trailing slash) or, for deterministic tests, an explicit literal loopback `http://IP:PORT` endpoint. Broker constraints must independently grant the exact effective authority and method.

GET sends `/posts/{postId}` with `Accept: application/json`. Create sends `/posts` with `Accept` and `Content-Type: application/json` and a body containing only `userId`, `title`, and `body`. Input and response byte limits, status mappings, echoed create fields, and transport-error redaction are enforced in the guest.

## The `placeholder` command word

```
placeholder posts get --post-id 7
placeholder posts create --user-id 1 --title "hello" --body "first post"
placeholder posts create --user-id 1 --title "hello" --body -   # reads piped input only after create authorization
placeholder --help                                               # rendered by the guest, at exit 0
```

The tree is the capability ID with the provider prefix swapped for the word: `placeholder posts get` proposes `jsonplaceholder.posts.get`. Each flag is the kebab-case of the wire field it fills, so `--post-id` is `postId`. `--help`, `--version`, and every usage error are rendered inside the component and authorize nothing. A well-formed argv becomes a *proposal* carrying exactly the input a direct invocation sends, and it travels the same constraint-set lookup and Cedar. ID ranges, byte limits, and the endpoint allowlist are checked by the same invoke path, so `--post-id 0` parses and is then refused as `invalid-input` before any HTTP. `--body -` proposes only the `body: "-"` and `stdinPiped: true` markers; no piped bytes enter the proposal or audit record. Once create is authorized, invoke reads at most 4097 bytes from broker stdio, rejects bodies over 4096 bytes, and then sends HTTP. Direct invoke and the word share validation. Successful invocations write exactly one JSON line to stdout; failures write stderr and return a nonzero status. A GET-only grant does not authorize create.

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

The SDK owns WIT bindings; this repository has no WIT mirror or manual wit-bindgen dependency. From a nested worktree use the actual sibling `provider-workflows/build.sh` checkout path instead of the relative path below. The provider pins `dekopon-provider-sdk = 0.36.0` and the real-component testkit `dekopon-provider-sdk-testkit = 0.36.0`. The deterministic build requires Rust 1.98.1 (`rustc 1.98.1 (48a229cea 2026-09-01)`) and `wasm-tools 1.259.0`.

```console
../provider-workflows/build.sh
```

The output is `jsonplaceholder-provider.wasm` plus its `.sha256`. The decoded component exports `dekopon:provider/provider@0.4.0`, imports exactly `dekopon:http/client@1.1.0` and `dekopon:stdio/streams@0.1.0`, and imports no WASI. An empty Wasmtime linker intentionally rejects it; execution requires the broker.

## Acceptance

Tests use injected native responses or literal loopback listeners only. They never contact the public JSONPlaceholder service.

```console
cargo fmt --all --check
cargo check --locked --workspace --all-targets --all-features
cargo check --locked --target wasm32-unknown-unknown
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked --package dekopon-jsonplaceholder-provider --lib --target wasm32-unknown-unknown -- -D warnings
cargo deny --all-features check bans licenses sources advisories
../provider-workflows/build.sh
core=target/wasm32-unknown-unknown/release/dekopon_jsonplaceholder_provider.wasm
test -s "$core"
wasm-tools validate "$core"
wasm-tools validate jsonplaceholder-provider.wasm
tmp=$(mktemp -d)
wasm-tools component wit jsonplaceholder-provider.wasm >"$tmp/component.wit"
grep -E '^[[:space:]]*import ' "$tmp/component.wit" | sed -E 's/^[[:space:]]*import ([^;]+);.*/\1/' >"$tmp/component-imports.txt"
if wasmtime run --invoke 'describe()' jsonplaceholder-provider.wasm >/dev/null 2>"$tmp/refusal.err"; then
  echo 'error: component instantiated under an empty linker' >&2; exit 1
fi
grep -F -f "$tmp/component-imports.txt" "$tmp/refusal.err" >/dev/null
rm -r "$tmp"
DEKOPON_PROVIDER_COMPONENT=$PWD/jsonplaceholder-provider.wasm cargo test --locked --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
```

`DEKOPON_PROVIDER_COMPONENT` must point at the built component; the broker-host tests panic without it. The shared `ci / validate` workflow in `dekopon-agents/provider-workflows` also generates a CycloneDX SBOM and verifies the release asset layout. The core Wasm must validate independently of the component. With pinned Wasmtime 48.0.2, the raw empty-linker smoke must refuse to instantiate the component and name a declared import. The second-build reproducibility comparison is currently paused.

## Release

Each version is released only by the tag workflow after explicit human authorization. See [RELEASE.md](RELEASE.md). Actions creates exactly `jsonplaceholder-provider.wasm` and `jsonplaceholder-provider.wasm.sha256`, provenance and CycloneDX attestations, a non-latest GitHub release, and one `ghcr.io/dekopon-agents/provider-jsonplaceholder:<version>` OCI manifest with one `application/wasm` layer. Do not create a remote, tag, release, or package by hand.

Licensed under MIT OR Apache-2.0. See the license files. The SBOM asset generated by the release workflow is the third-party disclosure; there is no `THIRD_PARTY_NOTICES.md` in the repository.
