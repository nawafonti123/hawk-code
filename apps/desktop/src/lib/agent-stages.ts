import type { AgentActivity, AgentState } from "@hawk-code/shared-types";
import type { PlanningPhase } from "../store/workbench";

/** The visible stages of HAWK's live reasoning. These are derived from real
 * agent activity — raw chain-of-thought is never shown. */
export type AgentStage =
  | "understanding"
  | "inspecting"
  | "planning"
  | "editing"
  | "verifying"
  | "responding"
  | "paused"
  | "failed"
  | "completed";

export const REASONING_STAGES: readonly AgentStage[] = [
  "understanding",
  "inspecting",
  "planning",
  "editing",
  "verifying",
] as const;

const RUNNING_TOOL_STAGES = new Set<string>([
  "list_files",
  "project_graph_structure",
  "project_graph_query",
  "read_file",
  "read_files",
  "replace_in_file",
  "write_file",
  "create_skill",
  "run_check",
  "git_status",
  "list_android_devices",
  "install_android_apk",
]);

export function isReasoningStage(stage: AgentStage): boolean {
  return (REASONING_STAGES as readonly string[]).includes(stage);
}

/** Maps the latest agent activity (and conversation state) to a stage. */
export function currentAgentStage(input: {
  agentState: AgentState;
  activities: AgentActivity[];
  hasContent: boolean;
  planningPhase: PlanningPhase;
  planningKickoff: boolean;
}): AgentStage {
  const {
    agentState,
    activities,
    hasContent,
    planningPhase,
    planningKickoff,
  } = input;
  if (agentState === "paused") return "paused";
  if (agentState === "failed") return "failed";
  if (agentState === "idle") return "completed";
  if (hasContent) return "responding";
  if (planningKickoff || planningPhase === "awaiting_answers") return "planning";
  const current =
    [...activities].reverse().find((activity) => activity.state === "running") ??
    activities.at(-1);
  if (!current || !RUNNING_TOOL_STAGES.has(current.tool)) return "understanding";
  if (current.state === "failed") return "failed";
  return stageForTool(current.tool);
}

function stageForTool(tool: string): AgentStage {
  if (tool === "replace_in_file" || tool === "write_file" || tool === "create_skill")
    return "editing";
  if (tool === "run_check" || tool === "git_status") return "verifying";
  return "inspecting";
}

/** The zero-based position of the active stage in the reasoning sequence. */
export function reasoningStageIndex(stage: AgentStage): number {
  const index = REASONING_STAGES.indexOf(stage);
  return index < 0 ? REASONING_STAGES.length - 1 : index;
}