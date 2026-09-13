# Release runbook

The tag workflow is the only publisher. Do not commit generated Wasm, create a release manually, push an OCI artifact by hand, or publish the private crate.

1. Obtain explicit human authorization for the exact version being released.
2. On clean current `main`, run the complete README acceptance commands, including native, component, broker, resource, dependency/license, and two-archive reproducibility gates.
3. Confirm `git status --short` is empty and the package version in `Cargo.toml` is the version being released.
4. Prove the `v<version>` tag and every draft/published release for it are absent, and prove `ghcr.io/dekopon-agents/provider-jsonplaceholder:<version>` is absent. Earlier versions stay published; stop rather than overwrite any state.
5. Create and push an **annotated** tag: `git tag -a v<version> -m 'v<version>' && git push origin v<version>`.

The workflow rejects a lightweight tag, tag/event/SHA/main mismatch, a tag that disagrees with the crate version, an existing release, an already-published OCI tag, wrong component interface, missing attestation, wrong bytes, or extra release/OCI content. Actions alone builds the bytes and creates exactly:

- `jsonplaceholder-provider.wasm`
- `jsonplaceholder-provider.wasm.sha256`

It creates build-provenance and CycloneDX SBOM attestations bound to the component digest. The SBOM is attestation input, not a release asset. It creates one run-marker-owned draft, captures immutable release/asset IDs, re-downloads and verifies those assets, then pushes only `ghcr.io/dekopon-agents/provider-jsonplaceholder:<version>` as `application/vnd.dekopon.provider.v1+wasm` with exactly one `application/wasm` layer titled `jsonplaceholder-provider.wasm`. It never creates `latest`, staging, or temporary tags.

GHCR container packages have no visibility API — `/orgs/*/packages/container/*` is GET, DELETE and POST-restore only — so the workflow asserts the package is public rather than making it public.

Immediately before publication it re-peels tag/main, verifies captured assets, WIT/imports, public commit-bound attestations, digest-pinned anonymous OCI bytes, and the captured package version and tag. Finalization changes only the captured release to `draft=false`, `prerelease=false`, `make_latest=false`; a credentials-free job then verifies public release and OCI bytes.

Failure/cancellation rollback acts only on captured immutable IDs/digests and the run marker. It deletes the exact package version/manifest this run pushed and the captured draft or finalized release, and deletes the package itself only when this run created it. Versions published by earlier releases are never taken offline. It fails loudly on cleanup errors. Attestations may remain as immutable evidence. Never recover by rebuilding or substituting bytes; any recovery requires a separate version/run/artifact/SHA/digest-pinned, explicitly authorized workflow.

Release notes must call out that `jsonplaceholder.posts.create` is an external write and that JSONPlaceholder's synthetic create response is not persisted.
