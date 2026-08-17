# ADR 0002: Local data and versioned IPC

- Status: Accepted
- Date: 2026-08-08

## Decision

Store local product state in SQLite using ordered transactional migrations. Every
frontend-to-core request includes `protocolVersion`; the Rust core rejects unknown
versions and validates path-bearing payloads before use.

## Consequences

Schema and IPC evolution become explicit. The application can migrate safely and
fail closed when a mismatched frontend or sidecar attempts communication.
