"""HAWK AI Modal deployment.

This service keeps the model and GPU entirely on Modal and exposes the small
OpenAI-compatible surface HAWK needs. The container scales to zero and is
limited to one personal container. We use llama.cpp for the GGUF build so the
L4 path can keep the model weights under the 24 GB VRAM target.

When the user attaches images, the backend calls the separate Hawk Vision
Modal deployment (Qwen2-VL-2B) to analyse them, then passes the textual
description to the local text-only Hawk K3 model for the final response.
"""

import json
import os
import time
from typing import Any

import modal

APP_NAME = "hawk-code-ai"
ENVIRONMENT_NAME = "hawk-code"
VOLUME_NAME = "hawk-code-model-cache"
MODEL_REPO = os.getenv(
    "HAWK_MODEL_REPO", "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF"
)
MODEL_FILE = os.getenv(
    "HAWK_MODEL_FILE", "Qwen3-Coder-30B-A3B-Instruct-Q3_K_M.gguf"
)
MODEL_ID = "qwen3-coder-30b-a3b-instruct"
DEFAULT_CONTEXT = 32_768
VISION_PROMPT = "Describe this image in detail for an AI coding assistant. Include: what the image shows (UI, diagram, error, code screenshot, etc.), any visible text/code, layout, colors, and any issues or elements the user might want to discuss. Be thorough but concise."

app = modal.App(APP_NAME)
model_volume = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)

image = (
    modal.Image.from_registry(
        "nvidia/cuda:12.6.3-devel-ubuntu22.04", add_python="3.12"
    )
    .apt_install("build-essential", "cmake", "git")
    .env(
        {
            "CMAKE_ARGS": (
                "-DGGML_CUDA=on "
                "-DGGML_BUILD_TESTS=OFF "
                "-DGGML_BUILD_EXAMPLES=OFF "
                "-DLLAMA_BUILD_TOOLS=OFF "
                "-DCMAKE_EXE_LINKER_FLAGS=-Wl,--allow-shlib-undefined"
            ),
            "CC": "gcc",
            "CXX": "g++",
        }
    )
    .pip_install(
        "fastapi==0.116.1",
        "huggingface-hub==0.34.4",
        "llama-cpp-python==0.3.16",
        "requests==2.32.3",
    )
)


def _call_vision_endpoint(image_url: str) -> str:
    """Call the Hawk Vision Modal deployment to analyse an image.

    Uses the internal Modal web endpoint. If unavailable, returns a fallback.
    """
    vision_url = os.getenv(
        "HAWK_VISION_URL",
        "https://mjakcon8--hawk-vision-hawkvision-web.modal.run/v1/chat/completions",
    )
    try:
        import requests as _requests

        payload = {
            "model": "Qwen/Qwen2-VL-2B-Instruct",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "image_url", "image_url": {"url": image_url}},
                        {"type": "text", "text": VISION_PROMPT},
                    ],
                }
            ],
            "max_tokens": 1024,
        }
        resp = _requests.post(vision_url, json=payload, timeout=60)
        resp.raise_for_status()
        data = resp.json()
        choices = data.get("choices", [])
        if choices:
            return choices[0].get("message", {}).get("content", "").strip()
        return "[Image attached — no description returned from vision model]"
    except Exception as exc:
        return f"[Image attached — vision analysis unavailable: {exc}]"


def _analyze_images_in_messages(
    messages: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """For each user message containing images, call Qwen-VL and replace
    image_url parts with the resulting text description.

    This allows the text-only Hawk K3 model to 'see' the image content
    through the vision model's analysis.
    """
    result: list[dict[str, Any]] = []
    for msg in messages:
        content = msg.get("content")
        if not isinstance(content, list) or msg.get("role") != "user":
            result.append(msg)
            continue
        text_parts: list[str] = []
        images: list[str] = []
        for part in content:
            if isinstance(part, dict):
                if part.get("type") == "text":
                    text_parts.append(part.get("text", ""))
                elif part.get("type") == "image_url":
                    url = (part.get("image_url") or {}).get("url", "")
                    if url:
                        images.append(url)
        if not images:
            result.append(msg)
            continue
        # Analyze each image through Hawk Vision endpoint
        descriptions: list[str] = []
        for idx, img_url in enumerate(images):
            desc = _call_vision_endpoint(img_url)
            label = f"Image {idx + 1}" if len(images) > 1 else "The attached image"
            descriptions.append(f"**{label}:**\n{desc}")
        vision_context = "\n\n".join(descriptions)
        original_text = "\n".join(text_parts).strip()
        if "vision analysis unavailable" in vision_context or "no description" in vision_context:
            enriched = (
                f"The user attached {len(images)} image(s), but the vision model could not "
                f"analyze them right now. Acknowledge the attached image(s), apologize for "
                f"the temporary limitation, and ask the user to try again.\n\n"
                f"[Original user message]\n{original_text}"
            ).strip()
        else:
            enriched = (
                f"The user attached {len(images)} image(s). The vision model (Qwen2-VL) "
                f"analyzed them and produced this description:\n\n{vision_context}\n\n"
                f"Use this description to answer the user's request.\n\n"
                f"[Original user message]\n{original_text}"
            ).strip()
        result.append({**msg, "content": enriched})
    return result


@app.cls(
    image=image,
    gpu="L4",
    volumes={"/root/.cache/huggingface": model_volume},
    min_containers=0,
    max_containers=1,
    buffer_containers=0,
    scaledown_window=60,
    timeout=900,
)
class HAWKModel:
    @modal.enter()
    def load(self) -> None:
        from huggingface_hub import hf_hub_download
        from llama_cpp import Llama

        model_path = hf_hub_download(
            repo_id=MODEL_REPO,
            filename=MODEL_FILE,
            cache_dir="/root/.cache/huggingface",
            token=os.getenv("HF_TOKEN") or None,
        )
        self.model = Llama(
            model_path=model_path,
            n_ctx=DEFAULT_CONTEXT,
            n_gpu_layers=-1,
            n_batch=512,
            chat_format="chatml",
            verbose=False,
        )

    @modal.asgi_app()
    def web(self) -> Any:
        from fastapi import FastAPI, Request
        from fastapi.responses import JSONResponse, StreamingResponse

        api = FastAPI(title="HAWK AI", docs_url="/docs")

        @api.post("/v1/chat/completions")
        async def completions(request: Request) -> Any:
            request = await request.json()
            messages = _analyze_images_in_messages(request.get("messages", []))
            stream = bool(request.get("stream", False))
            max_tokens = min(int(request.get("max_tokens", 16_384)), 16_384)
            started = time.time()

            def chunks():
                for chunk in self.model.create_chat_completion(
                    messages=messages,
                    max_tokens=max_tokens,
                    temperature=float(request.get("temperature", 0.2)),
                    stream=True,
                    tools=request.get("tools"),
                    tool_choice=request.get("tool_choice", "auto"),
                ):
                    yield f"data: {__import__('json').dumps(chunk)}\n\n"
                yield "data: [DONE]\n\n"

            if stream:
                return StreamingResponse(chunks(), media_type="text/event-stream")

            result = self.model.create_chat_completion(
                messages=messages,
                max_tokens=max_tokens,
                temperature=float(request.get("temperature", 0.2)),
                stream=False,
                tools=request.get("tools"),
                tool_choice=request.get("tool_choice", "auto"),
            )
            result.setdefault("model", MODEL_ID)
            result.setdefault("system_fingerprint", f"hawk-{int(started)}")
            return JSONResponse(result)

        @api.get("/v1/models")
        async def models(request: Request) -> Any:
            return JSONResponse(
                {
                    "object": "list",
                    "data": [
                        {
                            "id": MODEL_ID,
                            "object": "model",
                            "owned_by": "HAWK Studio",
                            "context_window": DEFAULT_CONTEXT,
                        }
                    ],
                }
            )

        return api

    # Keep a single OpenAI-compatible origin so the desktop client can append
    # /chat/completions and /models to a base URL ending in /v1.
    async def _legacy_completions(self, request: Any) -> Any:
        from fastapi.responses import JSONResponse, StreamingResponse
        from fastapi import Request

        if not isinstance(request, Request):
            raise TypeError("Modal did not provide a FastAPI request")
        request = await request.json()
        messages = _analyze_images_in_messages(request.get("messages", []))
        stream = bool(request.get("stream", False))
        max_tokens = min(int(request.get("max_tokens", 16_384)), 16_384)
        started = time.time()

        def chunks():
            for chunk in self.model.create_chat_completion(
                messages=messages,
                max_tokens=max_tokens,
                temperature=float(request.get("temperature", 0.2)),
                stream=True,
                tools=request.get("tools"),
                tool_choice=request.get("tool_choice", "auto"),
            ):
                yield f"data: {__import__('json').dumps(chunk)}\n\n"
            yield "data: [DONE]\n\n"

        if stream:
            return StreamingResponse(chunks(), media_type="text/event-stream")

        result = self.model.create_chat_completion(
            messages=messages,
            max_tokens=max_tokens,
            temperature=float(request.get("temperature", 0.2)),
            stream=False,
            tools=request.get("tools"),
            tool_choice=request.get("tool_choice", "auto"),
        )
        result.setdefault("model", MODEL_ID)
        result.setdefault("system_fingerprint", f"hawk-{int(started)}")
        return JSONResponse(result)

    async def _legacy_models(self, request: Any) -> Any:
        from fastapi.responses import JSONResponse
        from fastapi import Request

        if not isinstance(request, Request):
            raise TypeError("Modal did not provide a FastAPI request")
        return JSONResponse(
            {
                "object": "list",
                "data": [
                    {
                        "id": MODEL_ID,
                        "object": "model",
                        "owned_by": "HAWK Studio",
                        "context_window": DEFAULT_CONTEXT,
                    }
                ],
            }
        )


@app.local_entrypoint()
def main() -> None:
    print(f"Deploy {APP_NAME} in Modal environment {ENVIRONMENT_NAME}")
