# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.7.0] - 2026-10-08

### Changed

- Remove model-controlled `endpoint` inputs and `--endpoint` flags from both capabilities. Configure `providerSettings.jsonplaceholder.baseUrl` instead; it defaults to `https://jsonplaceholder.typicode.com`, preserves path prefixes, and leaves broker destination grants unchanged.
- Pin the provider SDK, testkit, and broker test dependencies to 0.38.0; validate owner settings before requests and add synthetic cassette replay coverage.

The `jsonplaceholder.posts.create` capability remains an external write. JSONPlaceholder returns a synthetic create response but does not persist the new post.

## [0.6.1] - 2026-10-08

### Changed

- Pin the provider SDK, testkit and broker dependencies to core 0.36.0; no provider behavior or WIT changes.

## [0.6.0] - 2026-10-06

### Changed

- Migrate JSONPlaceholder capabilities to SDK 0.34.0 and broker-owned stdout streams.
- Defer piped create body reads until authorized invocation and retain separate GET/POST grants.
- Migrate JSONPlaceholder to typed SDK 0.34.0 and provider@0.4.0 with newline-terminated stdout JSON.
- Defer piped create body reads until authorized invocation, preserving separate GET/POST authority.
- Verify that an unconfigured create capability cannot inherit read-only broker authority.
- Verify the JSON response and newline independent of test-only serde map ordering.
- Document raw-core validation and empty-linker refusal alongside component acceptance.
- Use portable whitespace matching in the empty-linker acceptance recipe.

The `jsonplaceholder.posts.create` capability remains an external write. JSONPlaceholder returns a synthetic create response but does not persist the new post.

## [0.5.0] - 2026-09-20

### Changed

- Move to provider SDK 0.18.0 and the HTTP 1.1.0 contract; no caller-facing behavior changes.
