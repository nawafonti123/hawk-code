"""Single source of truth for safe Modal deployment settings."""

GPU_POLICY = ("L4", "A10", "L40S")
MODAL_SETTINGS = {
    "environment": "hawk-code",
    "app": "hawk-code-ai",
    "volume": "hawk-code-model-cache",
    "min_containers": 0,
    "max_containers": 1,
    "buffer_containers": 0,
    "scaledown_window": 60,
    "default_context": 32768,
}
