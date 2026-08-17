import type { QwenModelId } from "@hawk-code/shared-types";

export function resolveChatModel(
  selectedModel: QwenModelId,
  baseUrl: string,
  hasImages: boolean,
): QwenModelId {
  if (!hasImages || selectedModel !== "qwen3.7-max") return selectedModel;
  try {
    const host = new URL(baseUrl).hostname;
    if (
      host === "dashscope.aliyuncs.com" ||
      host === "dashscope-intl.aliyuncs.com" ||
      host === "dashscope-us.aliyuncs.com"
    ) {
      return "qwen3.7-plus";
    }
  } catch {
    // Rust validates the provider URL and returns the user-facing error.
  }
  return selectedModel;
}
