import {
  DEFAULT_MODEL_ID,
  type AgentState,
  type AgentActivity,
  type ChatAttachment,
  type ChatMessage,
  type PermissionProfile,
  type PlanningQuestion,
  type HawkModelId,
  type ThemeMode,
  type UsageSummary,
  type UserProfile,
} from "@hawk-code/shared-types";
import { create } from "zustand";
import {
  clearCheckpoint,
  markCheckpointCompleted,
  markCheckpointInterrupted,
  saveCheckpoint,
  type CheckpointSnapshot,
} from "../lib/task-checkpoints";

export type RailView =
  | "tasks"
  | "files"
  | "git"
  | "agents"
  | "mcp"
  | "browser"
  | "settings";
export type AgentId = "coordinator" | "planner" | "code" | "review";
export type PlanningPhase = "kickoff" | "awaiting_answers" | "executing";

export interface PendingInstruction {
  id: string;
  text: string;
  createdAt: string;
}

interface WorkspaceState {
  authenticated: boolean;
  workspacePath: string | null;
  workspaceName: string | null;
  activeView: RailView;
  sidebarOpen: boolean;
  commandPaletteOpen: boolean;
  notice: string | null;
  composerDraft: string;
  browserAddress: string;
  attachments: ChatAttachment[];
  messages: ChatMessage[];
  conversationId: string;
  conversations: ConversationSession[];
  recentProjects: RecentProject[];
  conversationTitle: string;
  queuedMessageIds: string[];
  pendingInstructions: PendingInstruction[];
  agentState: AgentState;
  offline: boolean;
  activeModel: HawkModelId;
  hawkBaseUrl: string;
  permissionProfile: PermissionProfile;
  planFirst: boolean;
  planningPhase: PlanningPhase;
  theme: ThemeMode;
  userProfile: UserProfile;
  selectedAgent: AgentId;
  enabledSkills: string[];
  pendingSkillRequest: { skillId: string; reason: string } | null;
  usage: UsageSummary;
  setWorkspace: (path: string, name: string) => void;
  openGeneralChat: () => void;
  createConversation: () => void;
  selectConversation: (id: string) => void;
  renameConversation: (id: string, title: string) => void;
  deleteConversation: (id: string) => void;
  removeWorkspace: (path: string) => void;
  setActiveView: (view: RailView) => void;
  setSidebarOpen: (open: boolean) => void;
  setCommandPaletteOpen: (open: boolean) => void;
  setNotice: (notice: string | null) => void;
  setComposerDraft: (draft: string) => void;
  setBrowserAddress: (address: string) => void;
  addAttachments: (attachments: ChatAttachment[]) => void;
  removeAttachment: (id: string) => void;
  clearTask: () => void;
  addUserMessage: (content: string) => ChatMessage[];
  queueMessage: (id: string) => void;
  takeQueuedMessage: () => string | null;
  addPendingInstruction: (text: string) => void;
  updatePendingInstruction: (id: string, text: string) => void;
  removePendingInstruction: (id: string) => void;
  movePendingInstruction: (id: string, direction: "up" | "down") => void;
  clearPendingInstructions: () => void;
  beginAssistantMessage: () => string;
  updateAssistantActivity: (id: string, activity: AgentActivity) => void;
  appendAssistantDelta: (id: string, delta: string) => void;
  updateMessageContent: (id: string, content: string) => void;
  setAssistantPlanning: (
    id: string,
    content: string,
    questions: PlanningQuestion[],
  ) => void;
  finishAssistantMessage: (id: string, usage: UsageSummary) => void;
  failAssistantMessage: (id: string, error: string, continuable?: boolean) => void;
  setAgentState: (state: AgentState) => void;
  setOffline: (offline: boolean) => void;
  setActiveModel: (model: HawkModelId) => void;
  setHawkBaseUrl: (baseUrl: string) => void;
  setPermissionProfile: (profile: PermissionProfile) => void;
  setPlanFirst: (enabled: boolean) => void;
  setPlanningPhase: (phase: PlanningPhase) => void;
  setTheme: (theme: ThemeMode) => void;
  completeAuthentication: (profile: UserProfile) => void;
  logout: () => void;
  setSelectedAgent: (agent: AgentId) => void;
  toggleSkill: (skill: string) => void;
  requestSkill: (skillId: string, reason: string) => void;
  approveSkill: () => void;
  declineSkill: () => void;
}

const EMPTY_USAGE: UsageSummary = {
  promptTokens: 0,
  completionTokens: 0,
  totalTokens: 0,
};

interface SavedConversation {
  id: string;
  title: string;
  messages: ChatMessage[];
  createdAt: string;
}

type ConversationSession = SavedConversation;

interface ConversationCollection {
  activeId: string;
  conversations: ConversationSession[];
}

interface RecentProject {
  path: string;
  name: string;
}

function conversationKey(workspacePath: string | null): string {
  return `hawk.conversations.v2.${encodeURIComponent(workspacePath ?? "general")}`;
}

function createConversation(title = "New conversation"): ConversationSession {
  return {
    id: crypto.randomUUID(),
    title,
    messages: [],
    createdAt: new Date().toISOString(),
  };
}

function loadConversations(workspacePath: string | null): ConversationCollection {
  try {
    const raw = window.localStorage.getItem(conversationKey(workspacePath));
    if (!raw) {
      const legacy = workspacePath
        ? window.localStorage.getItem(
            `hawk.conversation.v1.${encodeURIComponent(workspacePath)}`,
          )
        : null;
      if (legacy) {
        const parsed = JSON.parse(legacy) as Partial<SavedConversation>;
        const conversation = createConversation(
          typeof parsed.title === "string" ? parsed.title : "Project conversation",
        );
        conversation.messages = Array.isArray(parsed.messages) ? parsed.messages : [];
        return { activeId: conversation.id, conversations: [conversation] };
      }
      const conversation = createConversation(
        workspacePath ? "Project conversation" : "General chat",
      );
      return { activeId: conversation.id, conversations: [conversation] };
    }
    const parsed = JSON.parse(raw) as Partial<ConversationCollection>;
    const conversations = Array.isArray(parsed.conversations)
      ? parsed.conversations.filter(
          (item): item is ConversationSession =>
            Boolean(item) &&
            typeof item.id === "string" &&
            typeof item.title === "string" &&
            Array.isArray(item.messages),
        )
      : [];
    if (conversations.length) {
      const activeId = conversations.some((item) => item.id === parsed.activeId)
        ? (parsed.activeId as string)
        : conversations[0]!.id;
      return { activeId, conversations };
    }
  } catch {
    // Local persistence must never block the workbench.
  }
  const conversation = createConversation(
    workspacePath ? "Project conversation" : "General chat",
  );
  return { activeId: conversation.id, conversations: [conversation] };
}

function saveConversations(
  workspacePath: string | null,
  activeId: string,
  conversations: ConversationSession[],
): void {
  try {
    window.localStorage.setItem(
      conversationKey(workspacePath),
      JSON.stringify({ activeId, conversations } satisfies ConversationCollection),
    );
  } catch {
    // A full local storage must not interrupt the active chat.
  }
}

function loadRecentProjects(): RecentProject[] {
  try {
    const parsed = JSON.parse(window.localStorage.getItem("hawk.projects.v1") ?? "[]") as unknown;
    return Array.isArray(parsed)
      ? parsed.filter(
          (item: unknown): item is RecentProject =>
            Boolean(item) &&
            typeof item === "object" &&
            typeof (item as RecentProject).path === "string" &&
            typeof (item as RecentProject).name === "string",
        )
      : [];
  } catch {
    return [];
  }
}

function saveRecentProjects(projects: RecentProject[]): void {
  try {
    window.localStorage.setItem("hawk.projects.v1", JSON.stringify(projects));
  } catch {
    // Local persistence must never block the workbench.
  }
}

function withActiveConversation(state: Pick<WorkspaceState, "conversationId" | "conversationTitle" | "messages" | "conversations">): ConversationSession[] {
  if (!state.conversationId) return state.conversations;
  return state.conversations.map((item) =>
    item.id === state.conversationId
      ? { ...item, title: state.conversationTitle, messages: state.messages }
      : item,
  );
}

function persistActiveConversation(state: Pick<WorkspaceState, "workspacePath" | "conversationId" | "conversationTitle" | "messages" | "conversations">): void {
  if (!state.conversationId) return;
  const conversations = withActiveConversation(state);
  saveConversations(state.workspacePath, state.conversationId, conversations);
}

function initialTheme(): ThemeMode {
  const saved = window.localStorage.getItem("hawk.preferences.v1");
  if (!saved) return "system";
  try {
    const value = JSON.parse(saved) as { theme?: ThemeMode };
    return value.theme === "light" || value.theme === "dark"
      ? value.theme
      : "system";
  } catch {
    return "system";
  }
}

function elapsedSince(createdAt: string): number {
  const startedAt = Date.parse(createdAt);
  return Number.isFinite(startedAt) ? Math.max(0, Date.now() - startedAt) : 0;
}

const EDIT_TOOLS = new Set(["replace_in_file", "write_file", "create_skill"]);

/** Recovery manager snapshot: keeps the running task resumable after any
 * interruption without persisting the full conversation (that lives in the
 * conversation store already). */
function checkpointSnapshot(
  state: Pick<
    WorkspaceState,
    | "conversationId"
    | "workspacePath"
    | "conversationTitle"
    | "messages"
    | "usage"
  >,
): CheckpointSnapshot {
  const assistantMessages = state.messages.filter(
    (message) => message.role === "assistant",
  );
  const active = assistantMessages.at(-1) ?? null;
  const stepCount =
    active?.activities?.filter((activity) => activity.state === "completed")
      .length ?? 0;
  const filesChanged = [
    ...new Set(
      (active?.activities ?? [])
        .filter(
          (activity) =>
            activity.state === "completed" &&
            EDIT_TOOLS.has(activity.tool) &&
            Boolean(activity.filePath),
        )
        .map((activity) => activity.filePath as string),
    ),
  ];
  return {
    conversationId: state.conversationId,
    workspacePath: state.workspacePath,
    title: state.conversationTitle,
    messages: state.messages.map(({ id, role }) => ({ id, role })),
    activeAssistantId: active?.id ?? null,
    stepCount,
    plan: null,
    filesChanged,
    partialContent:
      active && active.content ? active.content.slice(-20_000) : null,
    usage: state.usage,
  };
}

const INITIAL_GENERAL = loadConversations(null);
const INITIAL_GENERAL_ACTIVE =
  INITIAL_GENERAL.conversations.find(
    (item) => item.id === INITIAL_GENERAL.activeId,
  ) ?? INITIAL_GENERAL.conversations[0]!;

export const useWorkbenchStore = create<WorkspaceState>((set, get) => ({
  authenticated: false,
  workspacePath: null,
  workspaceName: null,
  activeView: "tasks",
  sidebarOpen: true,
  commandPaletteOpen: false,
  notice: null,
  composerDraft: "",
  browserAddress: "",
  attachments: [],
  messages: INITIAL_GENERAL_ACTIVE.messages,
  conversationId: INITIAL_GENERAL_ACTIVE.id,
  conversations: INITIAL_GENERAL.conversations,
  recentProjects: loadRecentProjects(),
  conversationTitle: INITIAL_GENERAL_ACTIVE.title,
  queuedMessageIds: [],
  pendingInstructions: [],
  agentState: "idle",
  offline: typeof navigator !== "undefined" && navigator.onLine === false,
  activeModel: DEFAULT_MODEL_ID,
  hawkBaseUrl:
    "https://mjakcon8-hawk-code--hawk-code-ai-hawkmodel-web.modal.run/v1",
  // The agent needs edit permission for normal coding requests. "auto"
  // allows safe workspace edits while still requiring separate approval for
  // actions classified as sensitive; users can switch to Ask or Full access.
  permissionProfile: "auto",
  planFirst: false,
  planningPhase: "kickoff",
  theme: initialTheme(),
  userProfile: {
    provider: "local",
    name: "Local user",
    email: null,
    avatarUrl: null,
  },
  selectedAgent: "coordinator",
  enabledSkills: [],
  pendingSkillRequest: null,
  usage: EMPTY_USAGE,
  setWorkspace: (workspacePath, workspaceName) => {
    const previous = get();
    persistActiveConversation(previous);
    const saved = loadConversations(workspacePath);
    const active = saved.conversations.find((item) => item.id === saved.activeId) ?? saved.conversations[0]!;
    const projects = [
      { path: workspacePath, name: workspaceName },
      ...previous.recentProjects.filter((item) => item.path !== workspacePath),
    ].slice(0, 24);
    saveRecentProjects(projects);
    set({
      workspacePath,
      workspaceName,
      activeView: "tasks",
      messages: active.messages,
      conversationId: active.id,
      conversations: saved.conversations,
      recentProjects: projects,
      conversationTitle: active.title,
      queuedMessageIds: [],
      attachments: [],
      composerDraft: "",
      notice: null,
      planningPhase: "kickoff",
    });
    clearCheckpoint();
  },
  openGeneralChat: () => {
    const previous = get();
    persistActiveConversation(previous);
    const saved = loadConversations(null);
    const active = saved.conversations.find((item) => item.id === saved.activeId) ?? saved.conversations[0]!;
    set({
      workspacePath: null,
      workspaceName: null,
      activeView: "tasks",
      messages: active.messages,
      conversationId: active.id,
      conversations: saved.conversations,
      conversationTitle: active.title,
      queuedMessageIds: [],
      attachments: [],
      composerDraft: "",
      planningPhase: "kickoff",
    });
    clearCheckpoint();
  },
  createConversation: () => {
    const state = get();
    persistActiveConversation(state);
    const conversation = createConversation(
      state.workspacePath ? "New project conversation" : "New chat",
    );
    const conversations = [conversation, ...withActiveConversation(state)];
    set({
      conversations,
      conversationId: conversation.id,
      conversationTitle: conversation.title,
      messages: [],
      queuedMessageIds: [],
      attachments: [],
      composerDraft: "",
      agentState: "idle",
      activeView: "tasks",
      planningPhase: "kickoff",
    });
    clearCheckpoint();
    saveConversations(state.workspacePath, conversation.id, conversations);
  },
  selectConversation: (id) => {
    const state = get();
    const currentConversations = withActiveConversation(state);
    const conversation = currentConversations.find((item) => item.id === id);
    if (!conversation || conversation.id === state.conversationId) return;
    persistActiveConversation(state);
    set({
      conversationId: conversation.id,
      conversationTitle: conversation.title,
      messages: conversation.messages,
      conversations: currentConversations,
      queuedMessageIds: [],
      attachments: [],
      composerDraft: "",
      agentState: "idle",
      activeView: "tasks",
      planningPhase: "kickoff",
    });
    clearCheckpoint();
    saveConversations(state.workspacePath, conversation.id, currentConversations);
  },
  renameConversation: (id, title) => {
    const trimmed = title.trim().slice(0, 80);
    if (!trimmed) return;
    const state = get();
    const conversations = state.conversations.map((item) =>
      item.id === id ? { ...item, title: trimmed } : item,
    );
    set({
      conversations,
      conversationTitle: id === state.conversationId ? trimmed : state.conversationTitle,
    });
    saveConversations(state.workspacePath, state.conversationId, conversations);
  },
  deleteConversation: (id) => {
    const state = get();
    const remaining = withActiveConversation(state).filter((item) => item.id !== id);
    const conversations = remaining.length ? remaining : [createConversation(state.workspacePath ? "Project conversation" : "General chat")];
    const next = conversations.find((item) => item.id === state.conversationId) ?? conversations[0]!;
    set({
      conversations,
      conversationId: next.id,
      conversationTitle: next.title,
      messages: next.messages,
      queuedMessageIds: [],
      attachments: [],
      composerDraft: "",
      agentState: "idle",
      planningPhase: "kickoff",
    });
    clearCheckpoint();
    saveConversations(state.workspacePath, next.id, conversations);
  },
  removeWorkspace: (path) => {
    const state = get();
    const recentProjects = state.recentProjects.filter((item) => item.path !== path);
    saveRecentProjects(recentProjects);
    // Removal is scoped to the HAWK project list only: the project folder and
    // its stored conversations stay untouched so re-adding the project
    // restores the previous chats.
    if (state.workspacePath === path) {
      state.openGeneralChat();
      set({ recentProjects });
    } else set({ recentProjects });
  },
  setActiveView: (activeView) => set({ activeView }),
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  setCommandPaletteOpen: (commandPaletteOpen) => set({ commandPaletteOpen }),
  setNotice: (notice) => set({ notice }),
  setComposerDraft: (composerDraft) => set({ composerDraft }),
  setBrowserAddress: (browserAddress) => set({ browserAddress }),
  addAttachments: (incoming) =>
    set((state) => ({
      attachments: [
        ...state.attachments,
        ...incoming.filter(
          (item) =>
            !state.attachments.some((current) => current.path === item.path),
        ),
      ].slice(0, 10),
    })),
  removeAttachment: (id) =>
    set((state) => ({
      attachments: state.attachments.filter(
        (attachment) => attachment.id !== id,
      ),
    })),
  clearTask: () => {
    get().createConversation();
  },
  addUserMessage: (content) => {
    const attachments = get().attachments;
    const message: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content,
      createdAt: new Date().toISOString(),
      attachments,
    };
    const messages = [...get().messages, message];
    const currentTitle = get().conversationTitle;
    const conversationTitle =
      get().messages.length === 0
        ? content.replace(/\s+/gu, " ").trim().slice(0, 52) || currentTitle
        : currentTitle;
    const conversations = get().conversations.map((item) =>
      item.id === get().conversationId
        ? { ...item, title: conversationTitle, messages }
        : item,
    );
    set({ messages, conversations, conversationTitle, composerDraft: "", attachments: [] });
    saveConversations(get().workspacePath, get().conversationId, conversations);
    return messages;
  },
  queueMessage: (id) =>
    set((state) => ({
      queuedMessageIds: state.queuedMessageIds.includes(id)
        ? state.queuedMessageIds
        : [...state.queuedMessageIds, id],
    })),
  takeQueuedMessage: () => {
    const [next, ...rest] = get().queuedMessageIds;
    if (!next) return null;
    set({ queuedMessageIds: rest });
    return next;
  },
  addPendingInstruction: (text) => {
    const trimmed = text.trim().slice(0, 4_000);
    if (!trimmed) return;
    const instruction: PendingInstruction = {
      id: crypto.randomUUID(),
      text: trimmed,
      createdAt: new Date().toISOString(),
    };
    set((state) => ({
      pendingInstructions: [...state.pendingInstructions, instruction],
    }));
  },
  updatePendingInstruction: (id, text) => {
    const trimmed = text.trim().slice(0, 4_000);
    if (!trimmed) return;
    set((state) => ({
      pendingInstructions: state.pendingInstructions.map((instruction) =>
        instruction.id === id ? { ...instruction, text: trimmed } : instruction,
      ),
    }));
  },
  removePendingInstruction: (id) =>
    set((state) => ({
      pendingInstructions: state.pendingInstructions.filter(
        (instruction) => instruction.id !== id,
      ),
    })),
  movePendingInstruction: (id, direction) =>
    set((state) => {
      const index = state.pendingInstructions.findIndex(
        (instruction) => instruction.id === id,
      );
      const target = direction === "up" ? index - 1 : index + 1;
      if (index < 0 || target < 0 || target >= state.pendingInstructions.length)
        return state;
      const pendingInstructions = [...state.pendingInstructions];
      const [moved] = pendingInstructions.splice(index, 1);
      pendingInstructions.splice(target, 0, moved!);
      return { pendingInstructions };
    }),
  clearPendingInstructions: () => set({ pendingInstructions: [] }),
  beginAssistantMessage: () => {
    const id = crypto.randomUUID();
    set((state) => ({
      agentState: "running",
      messages: [
        ...state.messages,
        {
          id,
          role: "assistant",
          content: "",
          createdAt: new Date().toISOString(),
        },
      ],
    }));
    saveCheckpoint(checkpointSnapshot(get()));
    return id;
  },
  updateAssistantActivity: (id, activity) => {
    set((state) => ({
      messages: state.messages.map((message) => {
        if (message.id !== id) return message;
        const activities = message.activities ?? [];
        const existing = activities.findIndex(
          (item) => item.id === activity.id,
        );
        return {
          ...message,
          activities:
            existing < 0
              ? [...activities, activity]
              : activities.map((item, index) =>
                  index === existing ? activity : item,
                ),
        };
      }),
    }));
    saveCheckpoint(checkpointSnapshot(get()));
  },
  appendAssistantDelta: (id, delta) => {
    set((state) => ({
      messages: state.messages.map((message) =>
        message.id === id
          ? {
              ...message,
              content: message.content + delta,
              thinkingDurationMs:
                message.thinkingDurationMs ?? elapsedSince(message.createdAt),
            }
          : message,
      ),
    }));
    saveCheckpoint(checkpointSnapshot(get()));
  },
  updateMessageContent: (id, content) => {
    set((state) => ({
      messages: state.messages.map((message) =>
        message.id === id ? { ...message, content } : message,
      ),
    }));
    persistActiveConversation(get());
  },
  setAssistantPlanning: (id, content, planningQuestions) =>
    set((state) => ({
      messages: state.messages.map((message) =>
        message.id === id
          ? { ...message, content, planningQuestions }
          : message,
      ),
    })),
  finishAssistantMessage: (id, eventUsage) => {
    set((state) => ({
      agentState: "idle",
      messages: state.messages.map((message) =>
        message.id === id
          ? {
              ...message,
              durationMs: elapsedSince(message.createdAt),
              thinkingDurationMs:
                message.thinkingDurationMs ?? elapsedSince(message.createdAt),
            }
          : message,
      ),
      usage: {
        promptTokens: state.usage.promptTokens + eventUsage.promptTokens,
        completionTokens:
          state.usage.completionTokens + eventUsage.completionTokens,
        totalTokens: state.usage.totalTokens + eventUsage.totalTokens,
      },
    }));
    persistActiveConversation(get());
    markCheckpointCompleted();
    clearCheckpoint();
  },
  failAssistantMessage: (id, error, continuable = false) => {
    set((state) => ({
      agentState: continuable ? "paused" : "failed",
      messages: state.messages.map((message) =>
        message.id === id
          ? {
              ...message,
              error,
              continuable: Boolean(continuable || message.continuable),
              durationMs: elapsedSince(message.createdAt),
              thinkingDurationMs:
                message.thinkingDurationMs ?? elapsedSince(message.createdAt),
            }
          : message,
      ),
    }));
    persistActiveConversation(get());
    saveCheckpoint(checkpointSnapshot(get()));
    if (continuable) markCheckpointInterrupted();
  },
  setAgentState: (agentState) => set({ agentState }),
  setOffline: (offline) => set({ offline }),
  setActiveModel: (activeModel) => set({ activeModel }),
  setHawkBaseUrl: (hawkBaseUrl) => set({ hawkBaseUrl }),
  setPermissionProfile: (permissionProfile) => set({ permissionProfile }),
  setPlanFirst: (planFirst) =>
    set({
      planFirst,
      planningPhase: planFirst ? "kickoff" : "executing",
    }),
  setPlanningPhase: (planningPhase) => set({ planningPhase }),
  setTheme: (theme) => {
    window.localStorage.setItem(
      "hawk.preferences.v1",
      JSON.stringify({ theme }),
    );
    set({ theme });
  },
  completeAuthentication: (userProfile) =>
    set({ authenticated: true, userProfile, notice: null }),
  logout: () => {
    set({
      authenticated: false,
      userProfile: {
        provider: "local",
        name: "Local user",
        email: null,
        avatarUrl: null,
      },
      messages: [],
      conversationTitle: "Project conversation",
      queuedMessageIds: [],
      attachments: [],
      composerDraft: "",
      workspacePath: null,
      workspaceName: null,
      activeView: "tasks",
      planningPhase: "kickoff",
    });
    clearCheckpoint();
  },
  setSelectedAgent: (selectedAgent) => set({ selectedAgent }),
  toggleSkill: (skill) =>
    set((state) => ({
      enabledSkills: state.enabledSkills.includes(skill)
        ? state.enabledSkills.filter((item) => item !== skill)
        : [...state.enabledSkills, skill],
    })),
  requestSkill: (skillId, reason) =>
    set({ pendingSkillRequest: { skillId, reason } }),
  approveSkill: () =>
    set((state) => {
      const req = state.pendingSkillRequest;
      if (!req) return {};
      return {
        enabledSkills: state.enabledSkills.includes(req.skillId)
          ? state.enabledSkills
          : [...state.enabledSkills, req.skillId],
        pendingSkillRequest: null,
      };
    }),
  declineSkill: () => set({ pendingSkillRequest: null }),
}));
