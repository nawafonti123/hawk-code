"""Prefetch Qwen3-Coder-Next-Base GGUF into the shared HAWK Modal volume."""

import os

import modal

APP_NAME = "hawk-code-base-prefetch"
VOLUME_NAME = "hawk-code-model-cache"
MODEL_REPO = os.getenv(
    "HAWK_MODEL_REPO",
    "mradermacher/Qwen3-Coder-Next-Base-GGUF",
)
MODEL_FILE = os.getenv(
    "HAWK_MODEL_FILE",
    "Qwen3-Coder-Next-Base.Q3_K_L.gguf",
)

app = modal.App(APP_NAME)
model_volume = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)
image = modal.Image.debian_slim(python_version="3.12").pip_install(
    "huggingface-hub==0.34.4"
)


@app.function(
    image=image,
    volumes={"/root/.cache/huggingface": model_volume},
    timeout=3600,
    memory=4096,
)
def prefetch() -> str:
    from huggingface_hub import hf_hub_download

    path = hf_hub_download(
        repo_id=MODEL_REPO,
        filename=MODEL_FILE,
        cache_dir="/root/.cache/huggingface",
        token=os.getenv("HF_TOKEN") or None,
    )
    model_volume.commit()
    return path


@app.local_entrypoint()
def main() -> None:
    print(f"Prefetching {MODEL_REPO}/{MODEL_FILE} ...")
    path = prefetch.remote()
    print(f"Cached successfully at {path}")
