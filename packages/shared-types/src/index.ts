export const IPC_PROTOCOL_VERSION = 1 as const;

export type IpcProtocolVersion = typeof IPC_PROTOCOL_VERSION;

export interface IpcRequest<TPayload> {
  protocolVersion: IpcProtocolVersion;
  requestId: string;
  payload: TPayload;
}

export interface RuntimeStatus {
  protocolVersion: IpcProtocolVersion;
  appVersion: string;
  platform: string;
  databaseReady: boolean;
}

export interface WorkspaceValidation {
  valid: boolean;
  canonicalPath: string;
  displayName: string;
}

export type PermissionProfile = "ask" | "auto" | "full";
export type ThemeMode = "system" | "dark" | "light";
export type AgentState = "idle" | "running" | "paused" | "failed";

export type HawkModelId =
  | "qwen3.7-max"
  | "qwen3.7-plus"
  | "qwen3.6-flash"
  | "qwen3-coder-30b-a3b-instruct";

/** @deprecated Use HawkModelId */
export type QwenModelId = HawkModelId;

export interface HawkModel {
  id: HawkModelId;
  displayName: string;
  mode: "quality" | "balanced" | "economy";
  contextWindow: number;
  maxOutputTokens: number;
  supportsTools: boolean;
  supportsReasoning: boolean;
  supportsStreaming: true;
}

/** @deprecated Use HawkModel */
export type QwenModel = HawkModel;

export const HAWK_MODELS: readonly HawkModel[] = [
  {
    id: "qwen3-coder-30b-a3b-instruct",
    displayName: "Hawk K3 · Coder",
    mode: "quality",
    contextWindow: 32_768,
    maxOutputTokens: 16_384,
    supportsTools: true,
    supportsReasoning: true,
    supportsStreaming: true,
  },
] as const;

/** @deprecated Use HAWK_MODELS */
export const QWEN_MODELS = HAWK_MODELS;

export const DEFAULT_MODEL_ID: HawkModelId = HAWK_MODELS[0]!.id;

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  createdAt: string;
  attachments?: ChatAttachment[];
  activities?: AgentActivity[];
  planningQuestions?: PlanningQuestion[];
  thinkingDurationMs?: number;
  durationMs?: number;
  error?: string;
  /** Set when the response stopped before completion (network interruption)
   * and the partial content can be continued without repeating. */
  continuable?: boolean;
}

export interface PlanningQuestion {
  id: string;
  question: string;
  options: string[];
}

export interface AgentActivity {
  id: string;
  tool: string;
  state: "running" | "completed" | "failed";
  detail: string;
  filePath?: string | null;
}

export interface ChatAttachment {
  id: string;
  name: string;
  path: string;
  mimeType: string;
  size: number;
  kind: "image" | "text" | "pdf";
  textContent?: string | undefined;
  dataUrl?: string | undefined;
}

export interface UserProfile {
  provider: "local" | "google" | "github" | "facebook";
  name: string;
  email: string | null;
  avatarUrl: string | null;
}

export interface UsageSummary {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

export interface ProviderStatus {
  configured: boolean;
  source: "environment" | "credential-manager" | "none";
  maskedKey: string | null;
}

export interface ProjectSummary {
  fileCount: number;
  directoryCount: number;
  frameworks: string[];
  truncated: boolean;
}

export interface GitStatus {
  branch: string;
  clean: boolean;
  entries: string[];
  fileCount: number;
  additions: number;
  deletions: number;
  files: GitFileChange[];
}

export interface GitFileChange {
  path: string;
  status: string;
  additions: number;
  deletions: number;
}

export interface GitFileDiff {
  path: string;
  patch: string;
  truncated: boolean;
}

export interface McpTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

export interface McpProbeResult {
  serverName: string;
  protocolVersion: string;
  tools: McpTool[];
}
