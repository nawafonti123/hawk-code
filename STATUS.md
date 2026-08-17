# HAWK Code — Status

Last updated: 2026-08-08

## Current milestone

**Phase 3 foundation — secure accounts, multimodal input, direct MCP, and internal browsing**

| Area                  | State                             | Evidence                                                                                   |
| --------------------- | --------------------------------- | ------------------------------------------------------------------------------------------ |
| Desktop shell         | Complete                          | Tauri 2 Rust application and native Windows build                                          |
| Workbench UI          | Complete                          | React shell, proper RTL/LTR, account popover, keyboard-accessible menus                    |
| Appearance            | Complete                          | System, dark, and light themes verified visually                                           |
| Workspace-scoped chat | Complete                          | Opening a folder resets and enters a conversation bound to its canonical path              |
| Attachments           | Complete for bounded images/text  | Native picker, Rust validation, type-labelled previews, and automatic long-paste text files |
| Voice input           | Complete                          | Live microphone analyser waveform plus continuous interim/final speech transcription       |
| Permission selection  | UI and policy context complete    | Ask, approve safe actions, and full-access profiles; tool enforcement remains Phase 3 work |
| Qwen provider         | Complete                          | Qwen 3.7 Max/Plus/Flash, streaming, metering, cancellation, Credential Manager             |
| Bundled MCP           | Complete for direct workspace use | Built-in stdio server, initialize/tools/list/tools/call, workspace summary and Git status   |
| Internal browser      | Complete                          | Native Tauri child webview with HTTP(S) navigation, reload, focus, resize, and close        |
| Browser extension     | Initial functional build          | MV3 active-tab capture, bounded DOM context, user-approved click/type, JSON export         |
| Language system       | Extensible                        | Large locale catalog, reviewed Arabic/English, validated Qwen generation and local import  |
| Local account         | Complete                          | First-launch gate, Argon2id, Windows Credential Manager, strict policy and lockout          |
| Social accounts       | Blocked by external configuration | Google/GitHub/Facebook each require registered HAWK OAuth client configuration              |

## Verification evidence

- TypeScript typecheck and ESLint pass across the workspace.
- React workbench tests pass: 11/11.
- Rust unit tests pass; live Qwen and live MCP integration tests pass when explicitly enabled.
- A real PNG was understood through the configured Alibaba endpoint. On legacy DashScope domains, image turns disclose and use Qwen 3.7 Plus because that deployment rejects visual content for the Max alias; text turns remain on the selected Max model.
- HAWK Browser Bridge Manifest V3 validation passes.
- First-launch authentication, password-strength feedback, and the light theme were inspected in the local browser with no console errors.

## Important boundaries

- The Browser Bridge is not yet paired to HAWK through an authenticated Native Messaging host. It does not claim autonomous background control.
- Bundled MCP tools run directly in the app; autonomous model tool invocation still waits for Phase 3 approval and audit enforcement.
- PDF and Office extraction are not enabled yet; accepting unsupported binaries would create a fake attachment experience.
- Arabic and English are reviewed packs. Other catalog languages are generated and schema-validated on demand through the configured Qwen provider, then persisted locally; they are not claimed as human-reviewed.
- Social sign-in cannot be completed without registering OAuth applications under HAWK Studio and supplying their client configuration.
