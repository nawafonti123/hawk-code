import type { PlanningQuestion } from "@hawk-code/shared-types";
import type { PlanningPhase } from "../store/workbench";

export const PLANNING_ANSWERS_EVENT = "hawk:planning-answers";

interface ParsedPlanningResponse {
  content: string;
  questions: PlanningQuestion[];
}

export function shouldUseWorkspaceAgent(
  _workspacePath: string | null,
  planFirst: boolean,
  planningPhase: PlanningPhase,
): boolean {
  // Every text turn goes through the desktop agent so capabilities such as
  // Playwright can be selected dynamically by tool_choice=auto when the user
  // actually asks for browser/computer work. Planning kickoff stays tool-free,
  // and image turns bypass this helper through the dedicated vision route.
  return !(planFirst && planningPhase === "kickoff");
}

export function extractPlanningQuestions(
  response: string,
): ParsedPlanningResponse {
  const block = /```hawk-questions\s*([\s\S]*?)```/iu.exec(response);
  if (!block) return { content: response.trim(), questions: [] };
  try {
    const parsed = JSON.parse(block[1]?.trim() ?? "") as {
      questions?: unknown;
    };
    if (!Array.isArray(parsed.questions)) throw new Error("invalid questions");
    const questions = parsed.questions
      .slice(0, 5)
      .map((value, index): PlanningQuestion | null => {
        if (!value || typeof value !== "object") return null;
        const candidate = value as {
          id?: unknown;
          question?: unknown;
          options?: unknown;
        };
        if (
          typeof candidate.question !== "string" ||
          !Array.isArray(candidate.options)
        )
          return null;
        const options = candidate.options
          .filter((option): option is string => typeof option === "string")
          .map((option) => option.trim())
          .filter(Boolean)
          .slice(0, 5);
        if (options.length < 2) return null;
        const fallbackId = `question-${index + 1}`;
        const id =
          typeof candidate.id === "string" && candidate.id.trim()
            ? candidate.id.trim().replace(/[^a-zA-Z0-9_-]/gu, "-")
            : fallbackId;
        return {
          id,
          question: candidate.question.trim(),
          options,
        };
      })
      .filter((question): question is PlanningQuestion => Boolean(question));
    return {
      content: response.replace(block[0], "").trim(),
      questions,
    };
  } catch {
    return { content: response.trim(), questions: [] };
  }
}

export function formatPlanningAnswers(
  questions: PlanningQuestion[],
  selections: Record<string, string>,
): string {
  return questions
    .map(
      (question, index) =>
        `${index + 1}. ${question.question}\n${selections[question.id] ?? ""}`,
    )
    .join("\n\n");
}
