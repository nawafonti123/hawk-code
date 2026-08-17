# HAWK Code — Master Plan

## Product direction

HAWK Code is a desktop-first AI engineering command center. It makes agent work
observable, permissioned, interruptible, and provable. The implementation follows
the authoritative Arabic product specification in
`docs/product/HAWK_CODE_MASTER_BUILD_PROMPT_AR_V3_APEX.md`.

## Delivery strategy

Work advances one verifiable vertical slice at a time. A phase is complete only
when its code, tests, documentation, and runnable evidence agree.

### Phase 0 — Discovery and plan

- [x] Inspect Node, pnpm, Rust, Cargo, Git, and .NET availability.
- [x] Record architecture decisions and security boundaries.
- [x] Record deferred features and risks.
- [x] Establish a phase status ledger.

### Phase 1 — Foundation

- [x] pnpm/Turborepo monorepo foundation.
- [x] Tauri 2 + Rust desktop shell.
- [x] React 19 + strict TypeScript + Vite workbench.
- [x] HAWK design tokens and a product-specific sub-brand.
- [x] SQLite schema and transactional migrations.
- [x] Workspace picker and settings surface.
- [x] Versioned, validated IPC skeleton.
- [x] CI baseline and local quality commands.

### Phase 2 — Providers and chat

- [x] Versioned provider IPC and SSE streaming protocol.
- [x] Qwen OpenAI-compatible adapter and trusted model registry.
- [x] Windows Credential Manager storage and provider settings.
- [x] Token usage metering and STOP ALL cancellation.
- [x] Live connection verification with `qwen3.7-max`.
- [ ] Extract the provider adapter into the packaged `agent-runtime` sidecar.
- [ ] Deterministic mock-provider streaming contract tests.

### Phase 3 — Agent, tools, and safety

- [x] Codex-style model and permission popovers with keyboard semantics.
- [x] Bounded image/source/text attachments through native selection and Rust validation.
- [x] Long clipboard payloads become type-labelled text attachments instead of flooding the composer.
- [x] Live microphone waveform and continuous interim/final speech transcription.
- [x] Planning-first chat policy with focused questions, explicit assumptions, and failure modes.
- [x] Live visual-input verification with an explicit legacy-endpoint fallback from Max to Plus.
- [x] Workspace-scoped conversation entry when a project opens.
- [x] Ask / approve-safe / full-access policy profiles in the workbench.
- [x] MCP 2025-11-25 stdio initialization and real tool discovery.
- [x] Bundled no-download HAWK MCP stdio server with workspace summary and Git status tools.
- [ ] Enforce permission decisions for every tool call in Rust and append audit events.
- [ ] Agent tool-call loop, approval queue, checkpoints, and process-tree cancellation.

### Phase 8 foundation — External browser bridge

- [x] Manifest V3 extension with active-tab-only permissions.
- [x] User-approved bounded page snapshot, selector click/type, and JSON export.
- [ ] Authenticated Native Messaging host and pairing flow.
- [ ] HAWK-driven browser commands, results, audit trail, and automated acceptance tests.

### Phase 7 foundation — Internal browser

- [x] Native Tauri child webview with URL validation and navigation controls.
- [x] Resize/focus/close lifecycle tied to the React workbench view.
- [ ] Browser history, downloads, per-site permissions, and audited agent navigation.

### Phase 15 prerequisite — Accounts

- [x] First-launch authentication gate and real local registration/login.
- [x] Strict password policy, Argon2id verifier storage in Windows Credential Manager, and temporary lockout.
- [ ] Register a HAWK Studio Google OAuth desktop application.
- [ ] Register HAWK Studio GitHub and Facebook OAuth applications.
- [ ] Implement provider-specific PKCE/authorization-code flows, secure refresh-token storage, and logout after client configuration exists.

### Later phases

Agent tools and safety, editor and Git, proof graph, internal browser, browser
bridge, Windows computer control, multi-agent routing, skills, missions, Apex
intelligence, gateway, collaboration, hardening, and release follow the phase
order in the master specification.

## Feature flags reserved

- `hawk.internalBrowser`
- `hawk.computerControl`
- `hawk.multiAgent`
- `hawk.proofGraph`
- `hawk.shadowWorkspace`
- `hawk.timeTravel`
- `hawk.apexLabs`

All are disabled until their owning phase has runnable verification.

## Known risks

- The .NET SDK is absent on the current machine; this affects the later Windows
  control host, not Phase 1.
- Desktop packaging depends on Windows WebView2 and native build prerequisites.
- Generated brand artwork must be reviewed by HAWK Studio before trademark use.
- Provider behavior, cost, and tool schemas require contract tests before Phase 2
  can be declared complete.
