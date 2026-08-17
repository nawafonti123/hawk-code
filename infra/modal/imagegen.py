"""HAWK ImageGen — Stable Diffusion XL image generation on Modal.

Generates images from text prompts using SDXL on a dedicated L4 GPU.
Exposes a simple /generate endpoint that the main Hawk K3 backend or
the frontend can call when the user asks to create or edit images.
"""

import io
import os
import time
import base64
from typing import Any

import modal

APP_NAME = "hawk-imagegen"

app = modal.App(APP_NAME)

gen_image = (
    modal.Image.debian_slim(python_version="3.11")
    .pip_install(
        "torch==2.4.1",
        "diffusers==0.31.0",
        "transformers==4.46.3",
        "accelerate==1.1.1",
        "fastapi==0.116.1",
        "pillow==11.0.0",
        "safetensors==0.4.5",
    )
)


@app.cls(
    image=gen_image,
    gpu="L4",
    min_containers=0,
    max_containers=1,
    scaledown_window=120,
    timeout=300,
)
class HawkImageGen:
    @modal.enter()
    def load(self) -> None:
        import torch
        from diffusers import StableDiffusionXLPipeline

        self.pipe = StableDiffusionXLPipeline.from_pretrained(
            "stabilityai/stable-diffusion-xl-base-1.0",
            torch_dtype=torch.float16,
            variant="fp16",
            use_safetensors=True,
        ).to("cuda")

    def _generate(self, prompt: str, width: int, height: int, steps: int) -> bytes:
        """Generate an image and return PNG bytes."""
        image = self.pipe(
            prompt=prompt,
            width=width,
            height=height,
            num_inference_steps=steps,
            guidance_scale=7.5,
        ).images[0]
        buf = io.BytesIO()
        image.save(buf, format="PNG")
        return buf.getvalue()

    @modal.asgi_app()
    def web(self) -> Any:
        from fastapi import FastAPI, Request
        from fastapi.responses import JSONResponse, Response

        api = FastAPI(title="HAWK ImageGen")

        @api.post("/generate")
        async def generate(request: Request) -> Any:
            body = await request.json()
            prompt = body.get("prompt", "")
            if not prompt:
                return JSONResponse(
                    {"error": "No prompt provided"}, status_code=400
                )
            width = min(int(body.get("width", 1024)), 1536)
            height = min(int(body.get("height", 1024)), 1536)
            steps = min(int(body.get("steps", 30)), 50)
            started = time.time()

            png_bytes = self._generate(prompt, width, height, steps)
            b64 = base64.b64encode(png_bytes).decode()
            elapsed = time.time() - started

            return JSONResponse(
                {
                    "image": f"data:image/png;base64,{b64}",
                    "elapsed_seconds": round(elapsed, 1),
                    "prompt": prompt,
                }
            )

        @api.get("/health")
        async def health() -> Any:
            return {"status": "ok", "model": "stable-diffusion-xl-base-1.0"}

        return api


@app.local_entrypoint()
def main() -> None:
    print(f"Deploy {APP_NAME} — SDXL on L4")
