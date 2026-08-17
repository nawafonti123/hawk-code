"""HAWK AI Modal deployment.

The GPU model stays on Modal and exposes an OpenAI-compatible surface for
HAWK Code. Images are analysed by the separate Hawk Vision deployment and
browser actions can be routed deterministically so obvious Playwright requests
do not spend minutes waiting for the model to decide which tool to call.
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
MODEL_REPO = os.getenv(
    "HAWK_MODEL_REPO", "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF"
)
MODEL_FILE = os.getenv(
    "HAWK_MODEL_FILE", "Qwen3-Coder-30B-A3B-Instruct-Q3_K_M.gguf"
)
MODEL_ID = "qwen3-coder-30b-a3b-instruct"
DEFAULT_CONTEXT = 32_768
WARM_IDLE_SECONDS = int(os.getenv("HAWK_WARM_IDLE_SECONDS", "180"))
PROMPT_BATCH = int(os.getenv("HAWK_PROMPT_BATCH", "1024"))
PROMPT_UBATCH = int(os.getenv("HAWK_PROMPT_UBATCH", "512"))
PROMPT_CACHE_BYTES = int(os.getenv("HAWK_PROMPT_CACHE_BYTES", str(2 << 30)))
# Normal agent routing should remain short. Browser requests that can be
# recognized safely never use this budget at all; they are routed directly.
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
            "are unavailable. Do not write prose before a tool call."
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
    """Fast-path obvious browser workflows without a model routing round.

    The user's request still gets a model-generated final answer after the real
    browser snapshot/screenshot results are available. We only remove the slow
    model decision about which deterministic browser action comes next.
    """
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
    gpu="L4",
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
        self.model = Llama(
            model_path=model_path,
            n_ctx=DEFAULT_CONTEXT,
            n_gpu_layers=-1,
            n_batch=PROMPT_BATCH,
            n_ubatch=PROMPT_UBATCH,
            offload_kqv=True,
            flash_attn=True,
            chat_format="chatml-function-calling",
            verbose=False,
        )
        self.model.set_cache(LlamaRAMCache(capacity_bytes=PROMPT_CACHE_BYTES))

    @modal.asgi_app()
    def web(self) -> Any:
        from fastapi import FastAPI, Request
        from fastapi.responses import JSONResponse, StreamingResponse

        api = FastAPI(title="HAWK AI", docs_url="/docs")

        @api.post("/v1/chat/completions")
        async def completions(request: Request) -> Any:
            request_data = await request.json()
            requested_tools = request_data.get("tools")
            raw_messages = _analyze_images_in_messages(request_data.get("messages", []))
            stream = bool(request_data.get("stream", False))

            # The desktop agent uses non-streaming calls for tool decisions.
            # Return deterministic browser tool calls instantly when possible.
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
            started = time.time()

            def chunks():
                for chunk in self.model.create_chat_completion(
                    messages=messages,
                    max_tokens=max_tokens,
                    temperature=float(request_data.get("temperature", 0.2)),
                    stream=True,
                    tools=effective_tools,
                    tool_choice=(
                        request_data.get("tool_choice", "auto")
                        if effective_tools
                        else None
                    ),
                ):
                    yield f"data: {json.dumps(chunk)}\n\n"
                yield "data: [DONE]\n\n"

            if stream:
                return StreamingResponse(chunks(), media_type="text/event-stream")

            result = self.model.create_chat_completion(
                messages=messages,
                max_tokens=max_tokens,
                temperature=float(request_data.get("temperature", 0.2)),
                stream=False,
                tools=effective_tools,
                tool_choice=(
                    request_data.get("tool_choice", "auto") if effective_tools else None
                ),
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

    async def _legacy_completions(self, request: Any) -> Any:
        from fastapi import Request
        from fastapi.responses import JSONResponse, StreamingResponse

        if not isinstance(request, Request):
            raise TypeError("Modal did not provide a FastAPI request")
        request_data = await request.json()
        requested_tools = request_data.get("tools")
        raw_messages = _analyze_images_in_messages(request_data.get("messages", []))
        stream = bool(request_data.get("stream", False))

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
        started = time.time()

        def chunks():
            for chunk in self.model.create_chat_completion(
                messages=messages,
                max_tokens=max_tokens,
                temperature=float(request_data.get("temperature", 0.2)),
                stream=True,
                tools=effective_tools,
                tool_choice=(
                    request_data.get("tool_choice", "auto") if effective_tools else None
                ),
            ):
                yield f"data: {json.dumps(chunk)}\n\n"
            yield "data: [DONE]\n\n"

        if stream:
            return StreamingResponse(chunks(), media_type="text/event-stream")

        result = self.model.create_chat_completion(
            messages=messages,
            max_tokens=max_tokens,
            temperature=float(request_data.get("temperature", 0.2)),
            stream=False,
            tools=effective_tools,
            tool_choice=(request_data.get("tool_choice", "auto") if effective_tools else None),
        )
        result.setdefault("model", MODEL_ID)
        result.setdefault("system_fingerprint", f"hawk-{int(started)}")
        return JSONResponse(result)

    async def _legacy_models(self, request: Any) -> Any:
        from fastapi import Request
        from fastapi.responses import JSONResponse

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
