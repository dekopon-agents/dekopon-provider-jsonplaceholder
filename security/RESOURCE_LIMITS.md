# Committed resource gates

The release component is exercised under independent broker-host ceilings:

| Resource | Fixed gate |
|---|---:|
| Wasm linear memory | 16,777,216 bytes |
| Fuel per fresh store | 64,000,000 units |
| Component artifact | 524,288 bytes |
| Provider success envelope | 65,536 bytes |
| HTTP requests per invocation | 1 |
| HTTP request bytes | 8,192 bytes |
| HTTP response bytes | 65,536 bytes |
| Broker timeout | 5,000 ms |

`tests/broker_host.rs::bounded_response_runs_under_committed_fuel_and_memory_ceilings` executes a maximum-valid response title/body under the fixed guest ceilings. Separate tests enforce exact authority/method, response-after-effect handling, and pre-network denial. CI reruns these tests with the pinned component and toolchain. Fuel is an execution budget, not a latency SLA.
