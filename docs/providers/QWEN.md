# Qwen provider

HAWK Code uses Alibaba Cloud Model Studio's OpenAI-compatible Chat Completions
API. The trusted local model registry currently contains:

- `qwen3.7-max` — default quality model and current flagship.
- `qwen3.7-plus` — balanced model.
- `qwen3.6-flash` — economy/low-latency model.

## Configure

1. Open **Settings → Qwen**.
2. Paste a new Alibaba Cloud Model Studio API key.
3. Confirm the Base URL for the key's region.
4. Select a model and press **Test connection**.

The key is stored in Windows Credential Manager under service
`com.hawkstudio.code`. Environment variables `QWEN_API_KEY` and
`DASHSCOPE_API_KEY` take precedence. The key is never written to SQLite,
browser storage, logs, or the React state tree.

Provider requests are rejected unless the destination uses HTTPS and belongs
to an official `aliyuncs.com` Model Studio endpoint. Streaming uses SSE and
STOP ALL cancels the active HTTP stream.
