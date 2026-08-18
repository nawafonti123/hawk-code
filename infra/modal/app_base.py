"""HAWK AI Modal runtime using Qwen3-Coder-Next-Base.

This runtime intentionally keeps the existing HAWK desktop protocol and API
surface stable while swapping only the model weights/runtime underneath it.
The previous infra/modal/app.py remains available as a rollback path.
"""

import json
import os
import re
import time
from typing import Any

import modal

APP_NAME = "hawk-code-ai"
ENVIRONMENT_NAME = "hawk-code"
VOLUME_NAME = "hawk-code-model-cache"

# Raw/base coder model. This is a quantization of Qwen/Qwen3-Coder-Next-Base;
# quantization changes storage precision, not the training stage of the model.
MODEL_REPO = os.getenv(
    "HAWK_MODEL_REPO",
    "mradermacher/Qwen3-Coder-Next-Base-GGUF",
)
MODEL_FILE = os.getenv(
    "HAWK_MODEL_FILE",
    "Qwen3-Coder-Next-Base.Q3_K_L.gguf",
)

# Keep the public API alias stable so the existing desktop build does not need
# a model migration. The actual loaded weights are Qwen3-Coder-Next-Base.
MODEL_ID = os.getenv("HAWK_MODEL_ID", "qwen3-coder-30b-a3b-instruct")
DEFAULT_CONTEXT = int(os.getenv("HAWK_CONTEXT", "32768"))
GPU_TYPE = os.getenv("HAWK_GPU", "L40S")
CPU_CORES = float(os.getenv("HAWK_CPU", "4"))
MEMORY_MB = int(os.getenv("HAWK_MEMORY_MB", "32768"))
WARM_IDLE_SECONDS = int(os.getenv("HAWK_WARM_IDLE_SECONDS", "180"))
PROMPT_BATCH = int(os.getenv("HAWK_PROMPT_BATCH", "512"))
PROMPT_UBATCH = int(os.getenv("HAWK_PROMPT_UBATCH", "256"))
PROMPT_CACHE_BYTES = int(os.getenv("HAWK_PROMPT_CACHE_BYTES", str(2 << 30)))
AGENT_ROUTE_MAX_TOKENS = int(os.getenv("HAWK_AGENT_ROUTE_MAX_TOKENS", "384"))
AGENT_FOLLOWUP_MAX_TOKENS = int(os.getenv("HAWK_AGENT_FOLLOWUP_MAX_TOKENS", "2048"))
BROWSER_FINAL_MAX_TOKENS = int(os.getenv("HAWK_BROWSER_FINAL_MAX_TOKENS", "1024"))

VISION_PROMPT = (
    "Describe this image in detail for an AI coding assistant. Include what the image "
    "shows, visible text/code, layout, colors, errors, and likely issues. Be thorough "
    "but concise."
)

app = modal.App(APP_NAME)
model_volume = modal.Volume.from_name(VOLUME_NAME, create_if_missing=True)

image = (
    modal.Image.from_registry(
        "nvidia/cuda:12.6.3-devel-ubuntu22.04",
        add_python="3.12",
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
        "llama-cpp-python==0.3.23",
        "requests==2.32.3",
    )
)


def _call_vision_endpoint(image_url: str) -> str:
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
        response = _requests.post(vision_url, json=payload, timeout=60)
        response.raise_for_status()
        data = response.json()
        choices = data.get("choices", [])
        if choices:
            return choices[0].get("message", {}).get("content", "").strip()
        return "[Image attached — no description returned from vision model]"
    except Exception as exc:
        return f"[Image attached — vision analysis unavailable: {exc}]"


def _analyze_images_in_messages(
    messages: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for message in messages:
        content = message.get("content")
        if not isinstance(content, list) or message.get("role") != "user":
            result.append(message)
            continue

        text_parts: list[str] = []
        images: list[str] = []
        for part in content:
            if not isinstance(part, dict):
                continue
            if part.get("type") == "text":
                text_parts.append(part.get("text", ""))
            elif part.get("type") == "image_url":
                url = (part.get("image_url") or {}).get("url", "")
                if url:
                    images.append(url)

        if not images:
            result.append(message)
            continue

        descriptions: list[str] = []
        for index, image_url in enumerate(images):
            description = _call_vision_endpoint(image_url)
            label = f"Image {index + 1}" if len(images) > 1 else "The attached image"
            descriptions.append(f"**{label}:**\n{description}")

        vision_context = "\n\n".join(descriptions)
        original_text = "\n".join(text_parts).strip()
        if "vision analysis unavailable" in vision_context or "no description" in vision_context:
            enriched = (
                f"The user attached {len(images)} image(s), but the vision model could not "
                f"analyze them right now. Acknowledge the temporary limitation and ask the "
                f"user to retry.\n\n[Original user message]\n{original_text}"
            ).strip()
        else:
            enriched = (
                f"The user attached {len(images)} image(s). Hawk Vision produced this "
                f"description:\n\n{vision_context}\n\nUse it to answer the user's request."
                f"\n\n[Original user message]\n{original_text}"
            ).strip()
        result.append({**message, "content": enriched})
    return result


def _prepare_agent_messages(
    messages: list[dict[str, Any]], tools: Any
) -> list[dict[str, Any]]:
    if not tools:
        return messages
    router_instruction = {
        "role": "system",
        "content": (
            "HAWK desktop tools are real and available on the user's computer. "
            "When the user asks for an action that matches a tool, call it immediately. "
            "Do not claim browsing, file access, project inspection, or listed capabilities "
            "are unavailable. Do not write prose before a tool call. "
            "When calling a tool, follow the tool-call format supplied by the active chat "
            "template exactly."
        ),
    }
    if messages and messages[0].get("role") == "system":
        return [messages[0], router_instruction, *messages[1:]]
    return [router_instruction, *messages]


def _request_max_tokens(
    request: dict[str, Any], messages: list[dict[str, Any]], tools: Any
) -> int:
    requested = min(int(request.get("max_tokens", 16_384)), 16_384)
    if not tools:
        return requested
    last_role = messages[-1].get("role") if messages else None
    cap = AGENT_FOLLOWUP_MAX_TOKENS if last_role == "tool" else AGENT_ROUTE_MAX_TOKENS
    return min(requested, cap)


def _tool_available(tools: Any, name: str) -> bool:
    if not isinstance(tools, list):
        return False
    for tool in tools:
        if not isinstance(tool, dict):
            continue
        function = tool.get("function")
        if isinstance(function, dict) and function.get("name") == name:
            return True
    return False


def _message_text(message: dict[str, Any]) -> str:
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for part in content:
            if isinstance(part, dict) and part.get("type") == "text":
                text = part.get("text")
                if isinstance(text, str):
                    parts.append(text)
        return "\n".join(parts)
    return ""


def _latest_user_turn(messages: list[dict[str, Any]]) -> tuple[int, str]:
    for index in range(len(messages) - 1, -1, -1):
        message = messages[index]
        if message.get("role") == "user":
            return index, _message_text(message)
    return -1, ""


def _last_browser_action_since(
    messages: list[dict[str, Any]], start_index: int
) -> str | None:
    for message in reversed(messages[start_index + 1 :]):
        if message.get("role") != "assistant":
            continue
        calls = message.get("tool_calls")
        if not isinstance(calls, list):
            continue
        for call in reversed(calls):
            if not isinstance(call, dict):
                continue
            function = call.get("function")
            if not isinstance(function, dict) or function.get("name") != "browser_control":
                continue
            raw_arguments = function.get("arguments", "{}")
            if isinstance(raw_arguments, str):
                try:
                    arguments = json.loads(raw_arguments)
                except Exception:
                    arguments = {}
            elif isinstance(raw_arguments, dict):
                arguments = raw_arguments
            else:
                arguments = {}
            action = arguments.get("action")
            return action if isinstance(action, str) else None
    return None


def _browser_intent(text: str) -> bool:
    lowered = text.lower()
    keywords = (
        "browser",
        "screenshot",
        "open http",
        "visit http",
        "navigate",
        "افتح",
        "المتصفح",
        "متصفح",
        "تصفح",
        "انتقل",
        "لقطة شاشة",
        "حلل الصفحة",
        "حلّل الصفحة",
    )
    return any(keyword in lowered for keyword in keywords)


def _wants_screenshot(text: str) -> bool:
    lowered = text.lower()
    return any(
        keyword in lowered
        for keyword in ("screenshot", "لقطة شاشة", "لقطه شاشه", "صورة للشاشة")
    )


def _extract_http_url(text: str) -> str | None:
    match = re.search(r"https?://[^\s<>\]\[(){}]+", text, flags=re.IGNORECASE)
    if not match:
        return None
    return match.group(0).rstrip(".,،؛;:!?؟\"'")


def _tool_call_completion(action: str, arguments: dict[str, Any]) -> dict[str, Any]:
    call_id = f"hawk-browser-{int(time.time() * 1000)}"
    return {
        "id": f"chatcmpl-{call_id}",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": MODEL_ID,
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": None,
                    "tool_calls": [
                        {
                            "id": call_id,
                            "type": "function",
                            "function": {
                                "name": "browser_control",
                                "arguments": json.dumps(
                                    {"action": action, **arguments},
                                    ensure_ascii=False,
                                ),
                            },
                        }
                    ],
                },
                "finish_reason": "tool_calls",
            }
        ],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
    }


def _direct_browser_tool_completion(
    messages: list[dict[str, Any]], tools: Any
) -> dict[str, Any] | None:
    if not _tool_available(tools, "browser_control"):
        return None

    user_index, user_text = _latest_user_turn(messages)
    if user_index < 0 or not _browser_intent(user_text):
        return None

    url = _extract_http_url(user_text)
    last_action = _last_browser_action_since(messages, user_index)

    if last_action is None:
        if not url:
            return None
        return _tool_call_completion("open", {"url": url})

    if last_action in {"open", "goto", "reload", "back", "forward"}:
        return _tool_call_completion("snapshot", {})

    if last_action == "snapshot" and _wants_screenshot(user_text):
        return _tool_call_completion("screenshot", {"fullPage": True})

    return None


def _browser_workflow_finished(messages: list[dict[str, Any]], tools: Any) -> bool:
    if not _tool_available(tools, "browser_control"):
        return False
    user_index, user_text = _latest_user_turn(messages)
    if user_index < 0 or not _browser_intent(user_text):
        return False
    last_action = _last_browser_action_since(messages, user_index)
    if last_action == "screenshot":
        return True
    return last_action == "snapshot" and not _wants_screenshot(user_text)


@app.cls(
    image=image,
    gpu=GPU_TYPE,
    cpu=CPU_CORES,
    memory=MEMORY_MB,
    volumes={"/root/.cache/huggingface": model_volume},
    min_containers=0,
    max_containers=1,
    buffer_containers=0,
    scaledown_window=WARM_IDLE_SECONDS,
    timeout=900,
)
class HAWKModel:
    @modal.enter()
    def load(self) -> None:
        from huggingface_hub import hf_hub_download
        from llama_cpp import Llama, LlamaRAMCache

        model_path = hf_hub_download(
            repo_id=MODEL_REPO,
            filename=MODEL_FILE,
            cache_dir="/root/.cache/huggingface",
            token=os.getenv("HF_TOKEN") or None,
        )

        # Do not force an Instruct-specific chat_format. llama-cpp-python will
        # use the chat template embedded in the GGUF metadata when available.
        self.model = Llama(
            model_path=model_path,
            n_ctx=DEFAULT_CONTEXT,
            n_gpu_layers=-1,
            n_batch=PROMPT_BATCH,
            n_ubatch=PROMPT_UBATCH,
            offload_kqv=True,
            flash_attn=True,
            verbose=False,
        )
        self.model.set_cache(LlamaRAMCache(capacity_bytes=PROMPT_CACHE_BYTES))

        architecture = self.model.metadata.get("general.architecture", "unknown")
        print(
            "HAWK raw model loaded:",
            MODEL_REPO,
            MODEL_FILE,
            f"architecture={architecture}",
            f"context={DEFAULT_CONTEXT}",
            f"gpu={GPU_TYPE}",
        )

    @modal.asgi_app()
    def web(self) -> Any:
        from fastapi import FastAPI, Request
        from fastapi.responses import JSONResponse, StreamingResponse

        api = FastAPI(title="HAWK AI — Qwen3 Coder Next Base", docs_url="/docs")

        @api.get("/health")
        async def health() -> Any:
            return JSONResponse(
                {
                    "ok": True,
                    "api_model": MODEL_ID,
                    "weights": "Qwen/Qwen3-Coder-Next-Base",
                    "quant_repo": MODEL_REPO,
                    "quant_file": MODEL_FILE,
                    "context_window": DEFAULT_CONTEXT,
                    "gpu": GPU_TYPE,
                }
            )

        @api.post("/v1/chat/completions")
        async def completions(request: Request) -> Any:
            request_data = await request.json()
            requested_tools = request_data.get("tools")
            raw_messages = _analyze_images_in_messages(request_data.get("messages", []))
            stream = bool(request_data.get("stream", False))

            # Preserve the existing deterministic fast path for obvious browser
            # actions so the raw model is not asked to route those unnecessarily.
            if not stream:
                direct = _direct_browser_tool_completion(raw_messages, requested_tools)
                if direct is not None:
                    return JSONResponse(direct)

            browser_finished = _browser_workflow_finished(raw_messages, requested_tools)
            effective_tools = None if browser_finished else requested_tools
            messages = _prepare_agent_messages(raw_messages, effective_tools)
            max_tokens = _request_max_tokens(request_data, messages, effective_tools)
            if browser_finished:
                max_tokens = min(max_tokens, BROWSER_FINAL_MAX_TOKENS)

            # Keep HAWK's existing deterministic coding defaults unless the
            # desktop explicitly sends different sampling values.
            temperature = float(request_data.get("temperature", 0.2))
            top_p = float(request_data.get("top_p", 0.95))
            top_k = int(request_data.get("top_k", 40))

            def completion_kwargs() -> dict[str, Any]:
                kwargs: dict[str, Any] = {
                    "messages": messages,
                    "max_tokens": max_tokens,
                    "temperature": temperature,
                    "top_p": top_p,
                    "top_k": top_k,
                }
                if effective_tools:
                    kwargs["tools"] = effective_tools
                    kwargs["tool_choice"] = request_data.get("tool_choice", "auto")
                return kwargs

            if stream:
                def chunks():
                    for chunk in self.model.create_chat_completion(
                        stream=True,
                        **completion_kwargs(),
                    ):
                        yield f"data: {json.dumps(chunk)}\n\n"
                    yield "data: [DONE]\n\n"

                return StreamingResponse(chunks(), media_type="text/event-stream")

            result = self.model.create_chat_completion(
                stream=False,
                **completion_kwargs(),
            )
            result.setdefault("model", MODEL_ID)
            result.setdefault("system_fingerprint", "hawk-qwen3-coder-next-base")
            return JSONResponse(result)

        @api.get("/v1/models")
        async def models() -> Any:
            return JSONResponse(
                {
                    "object": "list",
                    "data": [
                        {
                            "id": MODEL_ID,
                            "object": "model",
                            "owned_by": "HAWK Studio",
                            "context_window": DEFAULT_CONTEXT,
                            "underlying_model": "Qwen/Qwen3-Coder-Next-Base",
                            "training_stage": "pretraining",
                        }
                    ],
                }
            )

        return api


@app.local_entrypoint()
def main() -> None:
    print(
        f"Deploy {APP_NAME} in Modal environment {ENVIRONMENT_NAME} using "
        f"{MODEL_REPO}/{MODEL_FILE} on {GPU_TYPE}"
    )
