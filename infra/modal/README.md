# HAWK AI on Modal

The deployment keeps Qwen3-Coder on Modal GPU only. It uses an L4 first,
one container maximum, a 60-second scale-down window, and a persistent
`hawk-code-model-cache` Volume for Hugging Face weights.

## Deploy

```powershell
python -m modal token new
python -m modal deploy infra/modal/app.py --env hawk-code
```

Before deployment, create the secret once in the Modal dashboard or CLI with
`HF_TOKEN` (if the selected Hugging Face repo requires it).

The deployed endpoint is configured in HAWK as the endpoint URL with
`/chat/completions` and `/models` paths. Authentication is not required
for this personal deployment — the endpoint accepts all requests.
