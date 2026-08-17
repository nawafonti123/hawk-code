# HAWK AI — Modal setup

1. Install the Modal client: `python -m pip install -U modal`.
2. Authenticate once: `python -m modal token new`.
3. Create the Modal environment `hawk-code` and the secret `hawk-code-secrets`.
4. Put `HAWK_PROXY_TOKEN` in the secret. Add `HF_TOKEN` only if Hugging Face
   requires authentication for the chosen model repository.
5. Deploy with `python -m modal deploy infra/modal/app.py --env hawk-code`.
6. Copy the generated Web Function HTTPS endpoint into HAWK Settings and use
   the `/v1` base (the service exposes `/v1/models` and
   `/v1/chat/completions`). The current deployment URL is
   `https://mjakcon8-hawk-code--hawk-code-ai-hawkmodel-web.modal.run/v1`.
7. Save the same proxy token in HAWK's provider settings; it is stored in
   Windows Credential Manager, never in the renderer or source code.
8. Stop or remove the deployment from the Modal dashboard when it is no longer
   needed. The app is configured with `min_containers=0`, `max_containers=1`,
   and a 60-second scale-down window.

## Modes

- Economy: 16K context, 45–60 second scale-down.
- Balanced: 32K context (default), 60–90 second scale-down.
- Performance: 64K context, up to 120 seconds; only use if VRAM permits.

GPU policy is L4 first, then A10, then L40S only after an observed OOM or
unusable performance. Billing estimates must be treated as estimates; check
the Modal dashboard for the authoritative balance and spend limit.
