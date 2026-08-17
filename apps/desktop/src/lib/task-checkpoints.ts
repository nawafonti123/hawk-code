import type { UsageSummary } from "@hawk-code/shared-types";

export const CHECKPOINT_STORAGE_KEY = "hawk.checkpoint.v1";

/** UI event fired when the user chooses to resume an interrupted task. */
export const RESUME_TASK_EVENT = "hawk:resume-task";

export type CheckpointStatus = "active" | "interrupted" | "completed";

export interface TaskCheckpoint {
  conversationId: string;
  workspacePath: string | null;
  title: string;
  status: CheckpointStatus;
  /** Steps that reached a completed state in the running agent turn. */
  stepCount: number;
  plan: string | null;
  /** Files the agent edited or wrote during this task. */
  filesChanged: string[];
  partialContent: string | null;
  lastActivityAt: string;
  usage: UsageSummary;
}

export interface CheckpointSnapshot {
  conversationId: string;
  workspacePath: string | null;
  title: string;
  messages: readonly { id: string; role: "user" | "assistant" }[];
  activeAssistantId: string | null;
  stepCount: number;
  plan: string | null;
  filesChanged: string[];
  partialContent: string | null;
  usage: UsageSummary;
}

/** Persistence manager for the task state. Written after every activity and
 * delta so a crash or app close never loses more than the current step. */
export function saveCheckpoint(snapshot: CheckpointSnapshot): void {
  try {
    const checkpoint: TaskCheckpoint = {
      conversationId: snapshot.conversationId,
      workspacePath: snapshot.workspacePath,
      title: snapshot.title,
      status: "active",
      stepCount: snapshot.stepCount,
      plan: snapshot.plan,
      filesChanged: snapshot.filesChanged,
      partialContent: snapshot.partialContent,
      lastActivityAt: new Date().toISOString(),
      usage: snapshot.usage,
    };
    window.localStorage.setItem(
      CHECKPOINT_STORAGE_KEY,
      JSON.stringify(checkpoint),
    );
  } catch {
    // A full local storage must never interrupt the running task.
  }
}

export function markCheckpointInterrupted(): void {
  const current = loadCheckpoint();
  if (!current) return;
  try {
    window.localStorage.setItem(
      CHECKPOINT_STORAGE_KEY,
      JSON.stringify({ ...current, status: "interrupted" as const }),
    );
  } catch {
    // Persistence failures are ignored here as well.
  }
}

export function markCheckpointCompleted(): void {
  const current = loadCheckpoint();
  if (!current) return;
  try {
    window.localStorage.setItem(
      CHECKPOINT_STORAGE_KEY,
      JSON.stringify({ ...current, status: "completed" as const }),
    );
  } catch {
    // Persistence failures are ignored here as well.
  }
}

export function loadCheckpoint(): TaskCheckpoint | null {
  try {
    const raw = window.localStorage.getItem(CHECKPOINT_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<TaskCheckpoint>;
    if (
      typeof parsed.conversationId !== "string" ||
      (parsed.workspacePath !== null && typeof parsed.workspacePath !== "string")
    )
      return null;
    return {
      conversationId: parsed.conversationId,
      workspacePath: parsed.workspacePath ?? null,
      title: typeof parsed.title === "string" ? parsed.title : "",
      status:
        parsed.status === "active" ||
        parsed.status === "interrupted" ||
        parsed.status === "completed"
          ? parsed.status
          : "active",
      stepCount: Number.isFinite(parsed.stepCount) ? (parsed.stepCount ?? 0) : 0,
      plan: typeof parsed.plan === "string" ? parsed.plan : null,
      filesChanged: Array.isArray(parsed.filesChanged)
        ? parsed.filesChanged.filter(
            (item): item is string => typeof item === "string",
          )
        : [],
      partialContent:
        typeof parsed.partialContent === "string" ? parsed.partialContent : null,
      lastActivityAt:
        typeof parsed.lastActivityAt === "string"
          ? parsed.lastActivityAt
          : new Date().toISOString(),
      usage:
        parsed.usage && typeof parsed.usage === "object"
          ? {
              promptTokens: parsed.usage.promptTokens ?? 0,
              completionTokens: parsed.usage.completionTokens ?? 0,
              totalTokens: parsed.usage.totalTokens ?? 0,
            }
          : { promptTokens: 0, completionTokens: 0, totalTokens: 0 },
    };
  } catch {
    return null;
  }
}

export function clearCheckpoint(): void {
  try {
    window.localStorage.removeItem(CHECKPOINT_STORAGE_KEY);
  } catch {
    // Removing a key must never throw into the calling flow.
  }
}