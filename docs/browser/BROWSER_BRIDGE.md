# HAWK Browser Bridge

The unpacked Chromium extension is built at `apps/browser-extension/extension`.

Current verified capabilities:

- access is granted only for the active tab after the user opens the extension;
- capture page title, URL, selected text, headings, visible controls, and a bounded text excerpt;
- click a user-supplied CSS selector;
- type into a user-supplied input selector;
- export the latest bounded snapshot as JSON for attachment to HAWK Code.

The extension does not yet claim autonomous HAWK-to-browser control. That requires the separately packaged, authenticated Native Messaging host from Phase 8. Until that host is implemented and paired, the extension deliberately exposes no background host permissions and no remote-control channel.
