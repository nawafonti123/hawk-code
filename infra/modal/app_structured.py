"""HAWK Modal runtime with schema-constrained structured output support.

This keeps the same endpoint/model as app_base.py while forwarding
`response_format` into llama-cpp-python. HAWK Agent v7 uses this to make the
raw Qwen base model emit one valid JSON action instead of relying on unstable
provider-native tool-call markup.
"""

import json
import os
from typing import Any

import modal

from infra.modal import app_base as base

app = modal.App(base.APP_NAME)


@app.cls(
    image=base.image,
    gpu=base.GPU_TYPE,
    cpu=base.CPU_CORES,
    memory=base.MEMORY_MB,
    volumes={"/root/.cache/huggingface": base.model_volume},
    min_containers=0,
    max_containers=1,
    buffer_containers=0,
    scaledown_window=base.WARM_IDLE_SECONDS,
    timeout=900,
)
class HAWKModel:
    @modal.enter()
    def load(self) -> None:
        from huggingface_hub import hf_hub_download
        from llama_cpp import Llama, LlamaRAMCache

        model_path = hf_hub_download(
            repo_id=base.MODEL_REPO,
            filename=base.MODEL_FILE,
            cache_dir="/root/.cache/huggingface",
            token=os.getenv("HF_TOKEN") or None,
        )
        self.model = Llama(
            model_path=model_path,
            n_ctx=base.DEFAULT_CONTEXT,
            n_gpu_layers=-1,
            n_batch=base.PROMPT_BATCH,
            n_ubatch=base.PROMPT_UBATCH,
            offload_kqv=True,
            flash_attn=True,
            verbose=False,
        )
        self.model.set_cache(LlamaRAMCache(capacity_bytes=base.PROMPT_CACHE_BYTES))
        architecture = self.model.metadata.get("general.architecture", "unknown")
        print(
            "HAWK structured raw model loaded:",
            base.MODEL_REPO,
            base.MODEL_FILE,
            f"architecture={architecture}",
            f"context={base.DEFAULT_CONTEXT}",
            f"gpu={base.GPU_TYPE}",
        )

    @modal.asgi_app()
    def web(self) -> Any:
        from fastapi import FastAPI, Request
        from fastapi.responses import JSONResponse, StreamingResponse

        api = FastAPI(title="HAWK AI — Structured Qwen3 Coder Next Base", docs_url="/docs")

        @api.get("/health")
        async def health() -> Any:
            return JSONResponse(
                {
                    "ok": True,
                    "api_model": base.MODEL_ID,
                    "weights": "Qwen/Qwen3-Coder-Next-Base",
                    "quant_repo": base.MODEL_REPO,
                    "quant_file": base.MODEL_FILE,
                    "context_window": base.DEFAULT_CONTEXT,
                    "gpu": base.GPU_TYPE,
                    "structured_output": True,
                }
            )

        @api.post("/v1/chat/completions")
        async def completions(request: Request) -> Any:
            request_data = await request.json()
            requested_tools = request_data.get("tools")
            raw_messages = base._analyze_images_in_messages(request_data.get("messages", []))
            stream = bool(request_data.get("stream", False))

            if not stream:
                direct = base._direct_browser_tool_completion(raw_messages, requested_tools)
                if direct is not None:
                    return JSONResponse(direct)

            browser_finished = base._browser_workflow_finished(raw_messages, requested_tools)
            effective_tools = None if browser_finished else requested_tools
            messages = base._prepare_agent_messages(raw_messages, effective_tools)
            max_tokens = base._request_max_tokens(request_data, messages, effective_tools)
            if browser_finished:
                max_tokens = min(max_tokens, base.BROWSER_FINAL_MAX_TOKENS)

            temperature = float(request_data.get("temperature", 0.2))
            top_p = float(request_data.get("top_p", 0.95))
            top_k = int(request_data.get("top_k", 40))
            response_format = request_data.get("response_format")

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
                if isinstance(response_format, dict):
                    kwargs["response_format"] = response_format
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
            result.setdefault("model", base.MODEL_ID)
            result.setdefault("system_fingerprint", "hawk-qwen3-coder-next-base-structured")
            return JSONResponse(result)

        @api.get("/v1/models")
        async def models() -> Any:
            return JSONResponse(
                {
                    "object": "list",
                    "data": [
                        {
                            "id": base.MODEL_ID,
                            "object": "model",
                            "owned_by": "HAWK Studio",
                            "context_window": base.DEFAULT_CONTEXT,
                            "underlying_model": "Qwen/Qwen3-Coder-Next-Base",
                            "training_stage": "pretraining",
                            "structured_output": True,
                        }
                    ],
                }
            )

        return api


@app.local_entrypoint()
def main() -> None:
    print(
        f"Deploy {base.APP_NAME} structured runtime using "
        f"{base.MODEL_REPO}/{base.MODEL_FILE} on {base.GPU_TYPE}"
    )
