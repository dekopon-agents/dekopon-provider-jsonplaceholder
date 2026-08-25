# v0.1.0 release runbook

The tag workflow is the only publisher. Do not commit generated Wasm, create a release manually, push an OCI artifact by hand, or publish the private crate.

1. Obtain explicit human authorization for **v0.1.0 only**.
2. On clean current `main`, run the complete README acceptance commands, including native, component, broker, resource, dependency/license, and two-archive reproducibility gates.
3. Confirm `git status --short` is empty and package version is exactly `0.1.0`.
4. Prove the `v0.1.0` tag and every draft/published release for it are absent, and prove the entire `ghcr.io/dekopon-agents/provider-jsonplaceholder` package is absent. Stop rather than overwrite any state.
5. Create and push an **annotated** tag: `git tag -a v0.1.0 -m 'v0.1.0' && git push origin v0.1.0`.

The workflow rejects any other version, a lightweight tag, tag/event/SHA/main mismatch, version mismatch, existing release, existing package state, wrong component interface, missing attestation, wrong bytes, or extra release/OCI content. Actions alone builds the bytes and creates exactly:

- `jsonplaceholder-provider.wasm`
- `jsonplaceholder-provider.wasm.sha256`

It creates build-provenance and CycloneDX SBOM attestations bound to the component digest. The SBOM is attestation input, not a release asset. It creates one run-marker-owned draft, captures immutable release/asset IDs, re-downloads and verifies those assets, then pushes only `ghcr.io/dekopon-agents/provider-jsonplaceholder:0.1.0` as `application/vnd.dekopon.provider.v1+wasm` with exactly one `application/wasm` layer titled `jsonplaceholder-provider.wasm`. It never creates `latest`, staging, or temporary tags.

Immediately before publication it re-peels tag/main, verifies captured assets, WIT/imports, public commit-bound attestations, digest-pinned anonymous OCI bytes, and the sole package version/tag. Finalization changes only the captured release to `draft=false`, `prerelease=false`, `make_latest=false`; a credentials-free job then verifies public release and OCI bytes.

Failure/cancellation rollback acts only on captured immutable IDs/digests and the run marker. It hides public bytes first, deletes the exact package version/manifest and captured draft or finalized release, proves initial absence is restored, and fails loudly on cleanup errors. Attestations may remain as immutable evidence. Never recover by rebuilding or substituting bytes; any recovery requires a separate version/run/artifact/SHA/digest-pinned, explicitly authorized workflow.

Release notes must call out that `jsonplaceholder.posts.create` is non-idempotent external-write and that JSONPlaceholder's synthetic create response is not persisted.
