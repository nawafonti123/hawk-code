import type { ChatMessage } from "@hawk-code/shared-types";

interface StoredConversation {
  id?: unknown;
  title?: unknown;
  messages?: unknown;
}

interface StoredCollection {
  conversations?: unknown;
}

interface MemoryItem {
  id: string;
  createdAt: string;
  scope: string;
  title: string;
  content: string;
}

const STORAGE_PREFIX = "hawk.conversations.v2.";
const MAX_MEMORY_ITEMS = 18;
const MAX_MEMORY_CHARS = 7_500;
const MAX_ITEM_CHARS = 900;

/**
 * Builds a compact, local-only memory block from user messages stored in other
 * HAWK conversations. It deliberately remembers user-authored context only so
 * old assistant guesses never become durable facts.
 */
export function buildConversationMemoryContext(
  currentMessages: ChatMessage[],
): string {
  if (typeof window === "undefined") return "";
  const currentIds = new Set(currentMessages.map((message) => message.id));
  const items: MemoryItem[] = [];

  try {
    for (let index = 0; index < window.localStorage.length; index += 1) {
      const key = window.localStorage.key(index);
      if (!key?.startsWith(STORAGE_PREFIX)) continue;
      const raw = window.localStorage.getItem(key);
      if (!raw) continue;
      const parsed = JSON.parse(raw) as StoredCollection;
      if (!Array.isArray(parsed.conversations)) continue;
      const scope = decodeScope(key.slice(STORAGE_PREFIX.length));

      for (const candidate of parsed.conversations) {
        if (!candidate || typeof candidate !== "object") continue;
        const conversation = candidate as StoredConversation;
        const title =
          typeof conversation.title === "string"
            ? conversation.title.trim().slice(0, 100)
            : "Conversation";
        if (!Array.isArray(conversation.messages)) continue;

        for (const rawMessage of conversation.messages) {
          if (!rawMessage || typeof rawMessage !== "object") continue;
          const message = rawMessage as Partial<ChatMessage>;
          if (
            message.role !== "user" ||
            typeof message.id !== "string" ||
            currentIds.has(message.id) ||
            typeof message.content !== "string"
          )
            continue;
          const content = compact(message.content);
          if (!content) continue;
          items.push({
            id: message.id,
            createdAt:
              typeof message.createdAt === "string"
                ? message.createdAt
                : new Date(0).toISOString(),
            scope,
            title,
            content: content.slice(0, MAX_ITEM_CHARS),
          });
        }
      }
    }
  } catch {
    return "";
  }

  items.sort((left, right) =>
    right.createdAt.localeCompare(left.createdAt),
  );
  const selected = dedupe(items).slice(0, MAX_MEMORY_ITEMS).reverse();
  if (!selected.length) return "";

  const lines: string[] = [];
  let used = 0;
  for (const item of selected) {
    const line = `- [${item.scope} / ${item.title}] ${item.content}`;
    if (used + line.length > MAX_MEMORY_CHARS) break;
    lines.push(line);
    used += line.length;
  }
  if (!lines.length) return "";

  return [
    "HAWK cross-conversation memory (local user-authored context):",
    "Use these prior user statements only when relevant. Prefer the current request when it conflicts with older context. Do not claim a memory is current if the user has changed it.",
    ...lines,
  ].join("\n");
}

function dedupe(items: MemoryItem[]): MemoryItem[] {
  const seenIds = new Set<string>();
  const seenContent = new Set<string>();
  return items.filter((item) => {
    const contentKey = item.content.toLocaleLowerCase();
    if (seenIds.has(item.id) || seenContent.has(contentKey)) return false;
    seenIds.add(item.id);
    seenContent.add(contentKey);
    return true;
  });
}

function compact(value: string): string {
  return value
    .replace(/```[\s\S]*?```/gu, "[code omitted]")
    .replace(/\s+/gu, " ")
    .trim();
}

function decodeScope(encoded: string): string {
  try {
    const decoded = decodeURIComponent(encoded);
    return decoded === "general" ? "general" : decoded.split(/[\\/]/u).at(-1) || "project";
  } catch {
    return "conversation";
  }
}
