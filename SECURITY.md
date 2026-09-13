# Security

Report vulnerabilities privately through GitHub Security Advisories once the public repository exists. Do not put credentials, private URLs, response bodies, or exploit payloads in public issues.

## Trust boundary

This component is broker-only. It imports exactly `dekopon:http/client@1.0.0`; direct hosts intentionally refuse to load it. It has no WASI, sockets, filesystem, process, environment, clock, randomness, secrets, or ambient network authority.

The guest accepts only production JSONPlaceholder HTTPS or explicit literal loopback HTTP sockets. That validation is defense in depth, not authorization. Dekopon 0.15.0 remains authoritative for exact authority and method matching, DNS/destination controls, redirect behavior, one-call enforcement, deadlines, and byte limits. Production must keep `allowPlaintextLoopback: false`.

Read and create are distinct capabilities. Create remains an external write even though JSONPlaceholder returns a non-persistent synthetic record. A failure after POST may mean the effect executed.

The provider needs no credentials and exposes no credential input. Guest errors collapse response and transport details to stable secret-free messages. Broker audit/evidence may contain sanitized method, authority, status, and byte counts, but must exclude path, query, headers, body, inputs, outputs, and transport detail. All HTTP responses are untrusted content.
