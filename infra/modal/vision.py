"""HAWK Vision — Qwen2-VL image analysis on Modal.

Lightweight vision model deployed separately from the main Hawk K3 text model.
Runs on a single L4 GPU and exposes an OpenAI-compatible /v1/chat/completions
endpoint. The main Hawk K3 backend calls this endpoint when the user attaches
images so the text model can reason about the image content.
"""

import base64
import io
import time
from typing import Any

import modal

APP_NAME = "hawk-vision"
MODEL_ID = "Qwen/Qwen2-VL-2B-Instruct"
MAX_TOKENS = 1024

app = modal.App(APP_NAME)

vision_image = (
    modal.Image.debian_slim(python_version="3.11")
    .pip_install(
        "torch==2.4.1",
        "transformers==4.46.3",
        "accelerate==1.1.1",
        "fastapi==0.116.1",
        "pillow==11.0.0",
    )
)


@app.cls(
    image=vision_image,
    gpu="L4",
    min_containers=0,
    max_containers=1,
    scaledown_window=120,
    timeout=300,
)
class HawkVision:
    @modal.enter()
    def load(self) -> None:
        import torch
        from transformers import AutoProcessor, Qwen2VLForConditionalGeneration

        self.processor = AutoProcessor.from_pretrained(MODEL_ID)
        self.model = Qwen2VLForConditionalGeneration.from_pretrained(
            MODEL_ID,
            torch_dtype=torch.float16,
            device_map="auto",
        )

    def _describe_image(self, image_url: str, prompt: str) -> str:
        """Decode the data-URL image and ask Qwen2-VL to describe it."""
        import torch
        from PIL import Image

        if image_url.startswith("data:"):
            b64 = image_url.split(",", 1)[1]
            raw = base64.b64decode(b64)
            pil_image = Image.open(io.BytesIO(raw)).convert("RGB")
        else:
            import requests

            resp = requests.get(image_url, timeout=30)
            resp.raise_for_status()
            pil_image = Image.open(io.BytesIO(resp.content)).convert("RGB")

        messages = [
            {
                "role": "user",
                "content": [
                    {"type": "image", "image": pil_image},
                    {"type": "text", "text": prompt},
                ],
            }
        ]
        text = self.processor.apply_chat_template(
            messages, tokenize=False, add_generation_prompt=True
        )
        inputs = self.processor(
            text=[text],
            images=[pil_image],
            padding=True,
            return_tensors="pt",
        ).to(self.model.device)

        with torch.inference_mode():
            output_ids = self.model.generate(
                **inputs, max_new_tokens=MAX_TOKENS, temperature=0.2
            )
        trimmed = output_ids[0][inputs.input_ids.shape[1] :]
        return self.processor.decode(trimmed, skip_special_tokens=True)

    @modal.asgi_app()
    def web(self) -> Any:
        from fastapi import FastAPI, Request
        from fastapi.responses import JSONResponse

        api = FastAPI(title="HAWK Vision")

        @api.post("/v1/chat/completions")
        async def completions(request: Request) -> Any:
            body = await request.json()
            messages = body.get("messages", [])
            started = time.time()

            # Find images and prompt from the last user message
            image_urls: list[str] = []
            text_prompt = "Describe this image in detail."
            for msg in reversed(messages):
                if msg.get("role") != "user":
                    continue
                content = msg.get("content")
                if isinstance(content, list):
                    for part in content:
                        if isinstance(part, dict):
                            if part.get("type") == "image_url":
                                url = (part.get("image_url") or {}).get("url", "")
                                if url:
                                    image_urls.append(url)
                            elif part.get("type") == "text":
                                text_prompt = part.get("text", text_prompt)
                    break
                elif isinstance(content, str):
                    text_prompt = content
                    break

            if not image_urls:
                return JSONResponse(
                    {"error": "No images provided in the request"}, status_code=400
                )

            # Analyze each image
            descriptions: list[str] = []
            for idx, url in enumerate(image_urls):
                desc = self._describe_image(url, text_prompt)
                label = f"Image {idx + 1}" if len(image_urls) > 1 else "The image"
                descriptions.append(f"**{label}:**\n{desc}")

            result_text = "\n\n".join(descriptions)
            return JSONResponse(
                {
                    "id": f"hawk-vision-{int(started)}",
                    "object": "chat.completion",
                    "model": MODEL_ID,
                    "choices": [
                        {
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": result_text,
                            },
                            "finish_reason": "stop",
                        }
                    ],
                    "usage": {
                        "prompt_tokens": 0,
                        "completion_tokens": 0,
                        "total_tokens": 0,
                    },
                }
            )

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
                        }
                    ],
                }
            )

        return api


@app.local_entrypoint()
def main() -> None:
    print(f"Deploy {APP_NAME} — Qwen2-VL-2B vision model on L4")
