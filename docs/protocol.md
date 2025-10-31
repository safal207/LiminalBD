# LiminalDB Protocol v1

The v1 release freezes the public WebSocket and ABI surface used by official SDKs. All commands and events are described by JSON Schema files under `protocol/schemas`. Minor versions add backwards compatible fields; major versions may remove or rename operations.

## Versioning

- Current version: **1.0.0** (file `protocol/VERSION`).
- Minor updates may introduce optional fields or new commands/events.
- Major updates signal breaking changes to payloads or semantics.

## Transport

The canonical transport is WebSocket exchanging JSON envelopes:

```json
{
  "version": "1.0.0",
  "command": { "op": "lql", "id": "q-1", "query": "SELECT * FROM harmony" }
}
```

Servers reply with event envelopes shaped as:

```json
{
  "version": "1.0.0",
  "event": { "kind": "harmony", "id": "e-42", "ts": "2024-06-03T12:00:00Z", "score": 0.82 }
}
```

The list of legal commands and events, along with their required fields, is defined in JSON Schema for validation across SDKs.

## LQL grammar

A concise EBNF grammar describing the textual query language is provided in `protocol/lql.ebnf`. The generated AST interfaces in SDKs map one-to-one with the grammar production names. Identifiers are case-sensitive and string literals follow SQL-style quoting.

## Code generation

`protocol/codegen.js` and `protocol/Cargo.toml` + `codegen.rs` synthesize type-safe bindings:

- **TypeScript**: `sdk/ts/src/protocol-types.ts`
- **Rust**: `sdk/rust/src/protocol.rs`

These files expose typed command/event envelopes used by the SDK runtimes. Re-run the generators whenever schemas change.

## Validation and conformance

- JSON Schema validation should be performed for every inbound command and outbound event.
- The `conformance` crate emits a JUnit + Markdown report for the reference scenarios, ensuring that SDKs interoperate with the server.

## Rate limits and backpressure

Clients must obey per-namespace quotas. The official SDKs implement token buckets, bounded queues, and drop-oldest semantics. Servers respond with `quota` events when clients exceed limits.

## Error handling

Common failure surfaces include:

- Authentication failures (`auth` command missing or invalid).
- Command schema violations (reject with structured error event).
- Quota exhaustion (`alert` or `quota` events).
- Transport disconnects (clients should reconnect with exponential backoff).

## Migration guidance

Upgrading within the v1 minor series:

1. Regenerate SDK typings via `node protocol/codegen.js` and `cargo run -p liminaldb-protocol-codegen`.
2. Address any new optional fields surfaced in schemas.
3. Re-run the `liminaldb-conformance` suite against the updated SDK versions.

A future v2 would ship with migration notes in this document, preserving compatibility for existing v1 clients until deprecation is announced.

## Security reminders

- API keys carry namespace-scoped permissions (RBAC). Rotate secrets regularly.
- Quotas are enforced server-side; clients should probe limits via the `quota` command and heed `alert` events.
- Signed payloads (HMAC) are required for privileged operations (mirrors, noetic proposals). SDKs expose helpers to attach signatures.
