# Phase 1 Security Model

## Trust boundaries

1. The React webview is untrusted presentation logic.
2. Tauri runtime capabilities restrict plugin access to the main window.
3. Rust validates IPC versions, selected paths, and expected resource types.
4. SQLite is app-local and migrated transactionally.
5. Remote web content receives no Tauri capability.

## Phase 1 permissions

- Open a native directory picker.
- Load and modify the app-local SQLite database.
- Invoke the two enumerated application commands: runtime status and workspace
  validation.

No shell, process, unrestricted filesystem, HTTP, or secret-store capability is
granted in this phase.
