import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  IPC_PROTOCOL_VERSION,
  type AgentActivity,
  type ChatAttachment,
  type ChatMessage,
  type GitFileDiff,
  type GitStatus,
  type IpcRequest,
  type McpProbeResult,
  type ProjectSummary,
  type ProviderStatus,
  type QwenModelId,
  type RuntimeStatus,
  type UsageSummary,
  type UserProfile,
  type WorkspaceValidation,
} from "@hawk-code/shared-types";
import { z } from "zod";
import { buildConversationMemoryContext } from "./conversation-memory";

const runtimeStatusSchema = z.object({
  protocolVersion: z.literal(IPC_PROTOCOL_VERSION),
  appVersion: z.string(),
  platform: z.string(),
  databaseReady: z.boolean(),
});

const workspaceValidationSchema = z.object({
  valid: z.boolean(),
  canonicalPath: z.string(),
  displayName: z.string(),
});

const providerStatusSchema = z.object({
  configured: z.boolean(),
  source: z.enum(["environment", "credential-manager", "none"]),
  maskedKey: z.string().nullable(),
});

const usageSchema = z.object({
  promptTokens: z.number().nonnegative(),
  completionTokens: z.number().nonnegative(),
  totalTokens: z.number().nonnegative(),
});

const connectionSchema = z.object({
  model: z.string(),
  latencyMs: z.number().nonnegative(),
  usage: usageSchema,
});

const chatResultSchema = z.object({
  requestId: z.string(),
  model: z.string(),
  usage: usageSchema,
});

const projectSummarySchema = z.object({
  fileCount: z.number().nonnegative(),
  directoryCount: z.number().nonnegative(),
  frameworks: z.array(z.string()),
  truncated: z.boolean(),
});

const gitFileChangeSchema = z.object({
  path: z.string(),
  status: z.string(),
  additions: z.number().nonnegative(),
  deletions: z.number().nonnegative(),
});

const gitStatusSchema = z.object({
  branch: z.string(),
  clean: z.boolean(),
  entries: z.array(gitFileChangeSchema),
  fileCount: z.number().nonnegative(),
  additions: z.number().nonnegative(),
  deletions: z.number().nonnegative(),
  files: z.array(gitFileChangeSchema),
});

const gitFileDiffSchema = z.object({
  path: z.string(),
  patch: z.string(),
  truncated: z.boolean(),
});

const attachmentSchema = z.object({
  id: z.string(),
  name: z.string(),
  path: z.string(),
  mimeType: z.string(),
  size: z.number().nonnegative(),
  kind: z.enum(["image", "text"]),
  textContent: z
    .string()
    .nullish()
    .transform((value) => value ?? undefined),
  dataUrl: z
    .string()
    .nullish()
    .transform((value) => value ?? undefined),
});

const mcpProbeSchema = z.object({
  serverName: z.string(),
  protocolVersion: z.string(),
  tools: z.array(
    z.object({
      name: z.string(),
      description: z.string(),
      inputSchema: z.record(z.string(), z.unknown()),
    }),
  ),
});

const authProfileSchema = z.object({
  provider: z.enum(["local", "google", "github", "facebook"]),
  name: z.string(),
  email: z.string().nullable(),
  avatarUrl: z.string().nullable(),
});

const oauthProviderStatusSchema = z.array(
  z.object({
    provider: z.enum(["google", "github", "facebook"]),
    configured: z.boolean(),
  }),
);

export type OAuthProviderStatus = z.infer<
  typeof oauthProviderStatusSchema
>[number];

function request<TPayload>(payload: TPayload): IpcRequest<TPayload> {
  return {
    protocolVersion: IPC_PROTOCOL_VERSION,
    requestId: crypto.randomUUID(),
    payload,
  };
}

function systemPromptWithMemory(systemPrompt: string, messages: ChatMessage[]): string {
  const memory = buildConversationMemoryContext(messages);
  return memory ? `${systemPrompt}\n\n${memory}` : systemPrompt;
}

export function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function requireDesktop(): void {
  if (!isTauriRuntime())
    throw new Error("هذه الوظيفة تتطلب تطبيق HAWK Code المكتبي.");
}

export async function getRuntimeStatus(): Promise<RuntimeStatus | null> {
  if (!isTauriRuntime()) return null;
  return runtimeStatusSchema.parse(
    await invoke<unknown>("runtime_status", { request: request({}) }),
  );
}

export type PickWorkspaceResult =
  | { kind: "selected"; workspace: WorkspaceValidation }
  | { kind: "cancelled" }
  | { kind: "desktop-required" };

export async function pickWorkspace(): Promise<PickWorkspaceResult> {
  if (!isTauriRuntime()) return { kind: "desktop-required" };
  const selectedPath = await open({
    directory: true,
    multiple: false,
    title: "Open a workspace in HAWK Code",
  });
  if (!selectedPath) return { kind: "cancelled" };
  const response = await invoke<unknown>("validate_workspace", {
    request: request({ workspacePath: selectedPath }),
  });
  return {
    kind: "selected",
    workspace: workspaceValidationSchema.parse(response),
  };
}

export async function registerLocalAccount(input: {
  name: string;
  email: string;
  password: string;
}): Promise<UserProfile> {
  requireDesktop();
  return authProfileSchema.parse(
    await invoke<unknown>("auth_register", { request: request(input) }),
  );
}

export async function loginLocalAccount(input: {
  email: string;
  password: string;
}): Promise<UserProfile> {
  requireDesktop();
  return authProfileSchema.parse(
    await invoke<unknown>("auth_login", { request: request(input) }),
  );
}

export async function getOAuthProviderStatuses(): Promise<
  OAuthProviderStatus[]
> {
  if (!isTauriRuntime()) {
    return [
      { provider: "google", configured: false },
      { provider: "github", configured: false },
      { provider: "facebook", configured: false },
    ];
  }
  return oauthProviderStatusSchema.parse(
    await invoke<unknown>("oauth_status", { request: request({}) }),
  );
}

export async function loginWithGoogle(): Promise<UserProfile> {
  requireDesktop();
  return authProfileSchema.parse(
    await invoke<unknown>("oauth_login_google", { request: request({}) }),
  );
}

export async function pickAttachments(): Promise<ChatAttachment[]> {
  requireDesktop();
  const selected = await open({
    directory: false,
    multiple: true,
    title: "Attach files or images to HAWK Code",
    filters: [
      {
        name: "Images, code, and text",
        extensions: [
          "png",
          "jpg",
          "jpeg",
          "webp",
          "gif",
          "bmp",
          "svg",
          "txt",
          "md",
          "mdx",
          "log",
          "json",
          "yaml",
          "yml",
          "toml",
          "xml",
          "csv",
          "ts",
          "tsx",
          "js",
          "jsx",
          "mjs",
          "cjs",
          "css",
          "scss",
          "html",
          "rs",
          "py",
          "go",
          "java",
          "kt",
          "kts",
          "swift",
          "dart",
          "sh",
          "ps1",
          "bat",
          "cmd",
          "sql",
          "graphql",
          "vue",
          "svelte",
        ],
      },
    ],
  });
  if (!selected) return [];
  const paths = Array.isArray(selected) ? selected : [selected];
  const response = await invoke<unknown>("prepare_attachments", {
    request: request({ paths }),
  });
  return z.array(attachmentSchema).parse(response);
}

export async function pickMcpExecutable(): Promise<string | null> {
  requireDesktop();
  const selected = await open({
    directory: false,
    multiple: false,
    title: "Choose an MCP server executable",
  });
  return typeof selected === "string" ? selected : null;
}

export async function pickLanguagePackFile(): Promise<string | null> {
  requireDesktop();
  const selected = await open({
    directory: false,
    multiple: false,
    title: "Import a HAWK language pack",
    filters: [{ name: "HAWK language pack", extensions: ["json"] }],
  });
  if (typeof selected !== "string") return null;
  const response = z.array(attachmentSchema).parse(
    await invoke<unknown>("prepare_attachments", {
      request: request({ paths: [selected] }),
    }),
  );
  return response[0]?.textContent ?? null;
}

export async function probeMcpServer(config: {
  name: string;
  command: string;
  args: string[];
  workspacePath: string | null;
}): Promise<McpProbeResult> {
  requireDesktop();
  return mcpProbeSchema.parse(
    await invoke<unknown>("mcp_probe", { request: request(config) }),
  );
}

export async function probeBuiltInMcp(
  workspacePath: string | null,
): Promise<McpProbeResult> {
  requireDesktop();
  return mcpProbeSchema.parse(
    await invoke<unknown>("mcp_builtin_probe", {
      request: request({ workspacePath: workspacePath ?? "" }),
    }),
  );
}

export async function callBuiltInMcpTool(
  tool: string,
  workspacePath: string,
): Promise<Record<string, unknown>> {
  requireDesktop();
  return z.record(z.string(), z.unknown()).parse(
    await invoke<unknown>("mcp_builtin_call", {
      request: request({ tool, workspacePath }),
    }),
  );
}

export async function getQwenProviderStatus(): Promise<ProviderStatus> {
  requireDesktop();
  return providerStatusSchema.parse(
    await invoke<unknown>("qwen_provider_status", { request: request({}) }),
  );
}

export async function saveQwenApiKey(apiKey: string): Promise<ProviderStatus> {
  requireDesktop();
  return providerStatusSchema.parse(
    await invoke<unknown>("qwen_save_api_key", {
      request: request({ apiKey }),
    }),
  );
}

export async function deleteQwenApiKey(): Promise<boolean> {
  requireDesktop();
  return z
    .boolean()
    .parse(
      await invoke<unknown>("qwen_delete_api_key", { request: request({}) }),
    );
}

export interface QwenConfig {
  baseUrl: string;
  model: QwenModelId;
}

const agentActivitySchema = z.object({
  requestId: z.string(),
  id: z.string(),
  tool: z.string(),
  state: z.enum(["running", "completed", "failed"]),
  detail: z.string(),
  filePath: z.string().nullable().optional(),
});

export async function testQwenConnection(config: QwenConfig) {
  requireDesktop();
  return connectionSchema.parse(
    await invoke<unknown>("qwen_test_connection", { request: request(config) }),
  );
}

export async function streamQwenChat(
  config: QwenConfig,
  messages: ChatMessage[],
  systemPrompt: string,
  onDelta: (delta: string) => void,
): Promise<{ model: string; usage: UsageSummary }> {
  requireDesktop();
  const requestId = crypto.randomUUID();
  const enhancedSystemPrompt = systemPromptWithMemory(systemPrompt, messages);
  const unlisten = await listen<{ requestId: string; delta: string }>(
    "qwen://delta",
    (event) => {
      if (event.payload.requestId === requestId) onDelta(event.payload.delta);
    },
  );
  try {
    const response = await invoke<unknown>("qwen_chat", {
      request: request({
        requestId,
        config,
        messages: [
          { role: "system", content: enhancedSystemPrompt },
          ...messages.map(({ role, content, attachments }) => {
            if (!attachments?.length) return { role, content };
            const textFiles = attachments.filter(
              (item) => item.kind === "text",
            );
            const enrichedText = [
              content,
              ...textFiles.map(
                (file) =>
                  `\n\n--- Attached file: ${file.name} ---\n${file.textContent ?? ""}`,
              ),
            ].join("");
            const images = attachments.filter(
              (item) => item.kind === "image" && item.dataUrl,
            );
            if (!images.length) return { role, content: enrichedText };
            return {
              role,
              content: [
                { type: "text", text: enrichedText },
                ...images.map((image) => ({
                  type: "image_url",
                  image_url: { url: image.dataUrl },
                })),
              ],
            };
          }),
        ],
      }),
    });
    const result = chatResultSchema.parse(response);
    return { model: result.model, usage: result.usage };
  } finally {
    unlisten();
  }
}

export async function streamQwenAgent(
  config: QwenConfig,
  messages: ChatMessage[],
  systemPrompt: string,
  workspacePath: string,
  permissionProfile: "ask" | "auto" | "full",
  onDelta: (delta: string) => void,
  onActivity: (activity: AgentActivity) => void,
): Promise<{ model: string; usage: UsageSummary }> {
  requireDesktop();
  const requestId = crypto.randomUUID();
  const enhancedSystemPrompt = systemPromptWithMemory(systemPrompt, messages);
  const [unlistenDelta, unlistenActivity] = await Promise.all([
    listen<{ requestId: string; delta: string }>("qwen://delta", (event) => {
      if (event.payload.requestId === requestId) onDelta(event.payload.delta);
    }),
    listen<unknown>("agent://activity", (event) => {
      const parsed = agentActivitySchema.safeParse(event.payload);
      if (!parsed.success || parsed.data.requestId !== requestId) return;
      const activity: AgentActivity = {
        id: parsed.data.id,
        tool: parsed.data.tool,
        state: parsed.data.state,
        detail: parsed.data.detail,
        ...(parsed.data.filePath !== undefined
          ? { filePath: parsed.data.filePath }
          : {}),
      };
      onActivity(activity);
    }),
  ]);
  try {
    const response = await invoke<unknown>("qwen_agent", {
      request: request({
        requestId,
        config,
        workspacePath,
        permissionProfile,
        messages: [
          { role: "system", content: enhancedSystemPrompt },
          ...toProviderMessages(messages),
        ],
      }),
    });
    const result = chatResultSchema.parse(response);
    return { model: result.model, usage: result.usage };
  } finally {
    unlistenDelta();
    unlistenActivity();
  }
}

function toProviderMessages(messages: ChatMessage[]) {
  return messages.map(({ role, content, attachments }) => {
    if (!attachments?.length) return { role, content };
    const textFiles = attachments.filter((item) => item.kind === "text");
    const enrichedText = [
      content,
      ...textFiles.map(
        (file) =>
          `\n\n--- Attached file: ${file.name} ---\n${file.textContent ?? ""}`,
      ),
    ].join("");
    const images = attachments.filter(
      (item) => item.kind === "image" && item.dataUrl,
    );
    if (!images.length) return { role, content: enrichedText };
    return {
      role,
      content: [
        { type: "text", text: enrichedText },
        ...images.map((image) => ({
          type: "image_url",
          image_url: { url: image.dataUrl },
        })),
      ],
    };
  });
}

export async function stopAllTasks(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  return z
    .boolean()
    .parse(await invoke<unknown>("stop_all", { request: request({}) }));
}

export async function getWorkspaceSummary(
  workspacePath: string,
): Promise<ProjectSummary> {
  requireDesktop();
  return projectSummarySchema.parse(
    await invoke<unknown>("workspace_summary", {
      request: request({ workspacePath }),
    }),
  );
}

export async function getWorkspaceGitStatus(
  workspacePath: string,
): Promise<GitStatus> {
  requireDesktop();
  return gitStatusSchema.parse(
    await invoke<unknown>("workspace_git_status", {
      request: request({ workspacePath }),
    }),
  );
}

export async function getWorkspaceGitDiff(
  workspacePath: string,
  filePath: string,
): Promise<GitFileDiff> {
  requireDesktop();
  return gitFileDiffSchema.parse(
    await invoke<unknown>("workspace_git_diff", {
      request: request({ workspacePath, filePath }),
    }),
  );
}
