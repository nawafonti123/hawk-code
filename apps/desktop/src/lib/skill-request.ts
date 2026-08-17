export interface SkillRequest {
  skillId: string;
  reason: string;
}

const SKILL_REQUEST_PATTERN = /\[SKILL_REQUEST:\s*(\S+)\]\s*(.*)/;

export function parseSkillRequest(text: string): SkillRequest | null {
  const match = text.match(SKILL_REQUEST_PATTERN);
  if (!match) return null;
  return { skillId: match[1]!, reason: match[2]?.trim() ?? "" };
}
