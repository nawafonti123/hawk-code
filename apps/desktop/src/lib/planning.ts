import type { PlanningQuestion } from "@hawk-code/shared-types";
import type { PlanningPhase } from "../store/workbench";

export const PLANNING_ANSWERS_EVENT = "hawk:planning-answers";

interface ParsedPlanningResponse {
  content: string;
  questions: PlanningQuestion[];
}

export function shouldUseWorkspaceAgent(
  workspacePath: string | null,
  planFirst: boolean,
  planningPhase: PlanningPhase,
  forceDesktopAgent = false,
): boolean {
  return (
    (Boolean(workspacePath) || forceDesktopAgent) &&
    !(planFirst && planningPhase === "kickoff")
  );
}

/**
 * Keep ordinary general chat on the streaming chat path, but promote explicit
 * browser/computer interaction requests to the desktop agent. A project that
 * is already open is handled by shouldUseWorkspaceAgent regardless of text.
 */
export function shouldUseDesktopTools(content: string): boolean {
  const value = content.trim().toLocaleLowerCase();
  if (!value) return false;
  return /(?:playwright|browser|browse|navigate|open\s+(?:the\s+)?(?:site|website|browser|url)|click\s+(?:the\s+)?|fill\s+(?:the\s+)?|press\s+(?:the\s+)?|screenshot\s+(?:the\s+)?(?:page|site)|test\s+(?:the\s+)?(?:site|website)|افتح\s+(?:المتصفح|الموقع|الرابط)|فتح\s+(?:المتصفح|الموقع|الرابط)|تصفح|تصفّح|تنقل\s+(?:في|داخل)\s+(?:الموقع|المتصفح)|انتقل\s+(?:الى|إلى)\s+(?:الموقع|الرابط)|اضغط\s+(?:على\s+)?|إضغط\s+(?:على\s+)?|امل[اأ]\s+|اكتب\s+(?:في|داخل)\s+(?:الحقل|الموقع|المتصفح)|صور\s+(?:الصفحة|الموقع)|لقطة\s+شاشة\s+(?:للصفحة|للموقع)|اختبر\s+(?:الموقع|الصفحة)|تحكم\s+(?:في|ب)\s+(?:المتصفح|الموقع|الكمبيوتر|الحاسوب))/iu.test(
    value,
  );
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
