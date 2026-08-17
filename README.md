# HAWK Code

**AI Engineering Command Center** by HAWK Studio.

This repository is the Phase 1 foundation of HAWK Code: a Tauri 2 desktop shell,
a strict React/TypeScript workbench, a versioned IPC boundary, SQLite migrations,
workspace selection, and the first HAWK design-system implementation.

## Requirements

- Node.js 22.12 or newer
- pnpm 10
- Rust stable and Cargo
- Windows WebView2 runtime

The optional Windows automation host will require the .NET LTS SDK in a later
phase. It is not required for this foundation build.

## Run

```powershell
pnpm install
pnpm dev:web
pnpm dev:desktop
```

## Verify

```powershell
pnpm typecheck
pnpm lint
pnpm test
pnpm build
```

See [MASTER_PLAN.md](./MASTER_PLAN.md) and [STATUS.md](./STATUS.md) for scope and
delivery status.
"# hawk-code" 
