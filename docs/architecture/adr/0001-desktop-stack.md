# ADR 0001: Layered desktop stack

- Status: Accepted
- Date: 2026-08-08

## Decision

Use Tauri 2 and Rust as the trusted desktop boundary, React with strict TypeScript
for the webview, a future Node.js TypeScript sidecar for agent orchestration, and a
future .NET LTS sidecar for Windows UI Automation.

## Consequences

The React layer never launches operating-system processes directly. Privileged
operations cross a small, versioned, validated IPC boundary. This adds protocol
work but keeps permissions reviewable and replaceable by layer.
