import {
  ArrowUp,
  Check,
  ChevronDown,
  FileCode2,
  ListChecks,
  Mic,
  MicOff,
  Paperclip,
  Play,
  Plus,
  Shield,
  Square,
  X,
  AlertTriangle,
  CheckCircle,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { HAWK_MODELS, type PermissionProfile, type HawkModelId } from "@hawk-code/shared-types";
import {
  pickAttachments,
  stopAllTasks,
  streamQwenAgent,
  streamQwenChat,
} from "../lib/ipc";
import { resolveChatModel } from "../lib/qwen-model";
import {
  extractPlanningQuestions,
  PLANNING_ANSWERS_EVENT,
  shouldUseWorkspaceAgent,
} from "../lib/planning";
import { RESUME_TASK_EVENT } from "../lib/task-checkpoints";
import { useVoiceInput } from "../lib/useVoiceInput";
import { useWorkbenchStore } from "../store/workbench";
import { PopoverMenu } from "./PopoverMenu";

const PERMISSIONS: ReadonlyArray<{
  id: PermissionProfile;
  titleKey: string;
  detailKey: string;
}> = [
  {
    id: "ask",
    titleKey: "permissions.ask",
    detailKey: "permissions.askDetail",
  },
  {
    id: "auto",
    titleKey: "permissions.auto",
    detailKey: "permissions.autoDetail",
  },
  {
    id: "full",
    titleKey: "permissions.full",
    detailKey: "permissions.fullDetail",
  },
];

const DANGEROUS_TOOLS = new Set([
  "replace_in_file",
  "write_file",
  "create_skill",
  "run_check",
  "install_android_apk",
]);

const SLASH_COMMANDS = [
  { value: "/review", labelKey: "slash.review" },
  { value: "/settings", labelKey: "slash.settings" },
  { value: "/theme dark", labelKey: "slash.theme" },
  { value: "/language", labelKey: "slash.language" },
  { value: "/browser", labelKey: "slash.browser" },
  { value: "/mcp", labelKey: "slash.mcp" },
  { value: "/new", labelKey: "slash.new" },
] as const;

export type RunMode = "normal" | "continue" | "followup" | "resume";

export function Composer() {
  const { i18n, t } = useTranslation();
  const composerWrapRef = useRef<HTMLDivElement>(null);
  const [attaching, setAttaching] = useState(false);
  const draft = useWorkbenchStore((state) => state.composerDraft);
  const attachments = useWorkbenchStore((state) => state.attachments);
  const agentState = useWorkbenchStore((state) => state.agentState);
  const offline = useWorkbenchStore((state) => state.offline);
  const activeModel = useWorkbenchStore((state) => state.activeModel);
  const [thinkingMode, setThinkingMode] = useState<"fast" | "balanced" | "deep">("deep");
  const [modelPower, setModelPower] = useState<"economy" | "quality">("quality");
  const hawkBaseUrl = useWorkbenchStore((state) => state.hawkBaseUrl);
  const permission = useWorkbenchStore((state) => state.permissionProfile);
  const planFirst = useWorkbenchStore((state) => state.planFirst);
  const planningPhase = useWorkbenchStore((state) => state.planningPhase);
  const selectedAgent = useWorkbenchStore((state) => state.selectedAgent);
  const enabledSkills = useWorkbenchStore((state) => state.enabledSkills);
  const workspaceName = useWorkbenchStore((state) => state.workspaceName);
  const workspacePath = useWorkbenchStore((state) => state.workspacePath);
  const setDraft = useWorkbenchStore((state) => state.setComposerDraft);
  const setActiveView = useWorkbenchStore((state) => state.setActiveView);
  const setBrowserAddress = useWorkbenchStore(
    (state) => state.setBrowserAddress,
  );
  const addAttachments = useWorkbenchStore((state) => state.addAttachments);
  const removeAttachment = useWorkbenchStore((state) => state.removeAttachment);
  const addUserMessage = useWorkbenchStore((state) => state.addUserMessage);
  const queueMessage = useWorkbenchStore((state) => state.queueMessage);
  const beginAssistant = useWorkbenchStore(
    (state) => state.beginAssistantMessage,
  );
  const appendDelta = useWorkbenchStore((state) => state.appendAssistantDelta);
  const setAssistantPlanning = useWorkbenchStore(
    (state) => state.setAssistantPlanning,
  );
  const updateActivity = useWorkbenchStore(
    (state) => state.updateAssistantActivity,
  );
  const finishAssistant = useWorkbenchStore(
    (state) => state.finishAssistantMessage,
  );
  const failAssistant = useWorkbenchStore(
    (state) => state.failAssistantMessage,
  );
  const setAgentState = useWorkbenchStore((state) => state.setAgentState);
  const setOffline = useWorkbenchStore((state) => state.setOffline);
  const addPendingInstruction = useWorkbenchStore(
    (state) => state.addPendingInstruction,
  );
  const setActiveModel = useWorkbenchStore((state) => state.setActiveModel);
  const setPermission = useWorkbenchStore(
    (state) => state.setPermissionProfile,
  );
  const setPlanFirst = useWorkbenchStore((state) => state.setPlanFirst);
  const setPlanningPhase = useWorkbenchStore((state) => state.setPlanningPhase);
  const setTheme = useWorkbenchStore((state) => state.setTheme);
  const clearTask = useWorkbenchStore((state) => state.clearTask);
  const toggleSkill = useWorkbenchStore((state) => state.toggleSkill);
  const setNotice = useWorkbenchStore((state) => state.setNotice);
  const handleVoiceTranscript = useCallback(
    (text: string) => setDraft(text),
    [setDraft],
  );
  const handleVoiceError = useCallback(
    (message: string) => {
      if (message === "VOICE_RECOGNITION_UNAVAILABLE")
        setNotice(t("composer.voiceUnavailable"));
      else if (message === "MICROPHONE_UNAVAILABLE")
        setNotice(t("composer.microphoneUnavailable"));
      else setNotice(t("composer.voiceError", { error: message }));
    },
    [setNotice, t],
  );
  const voice = useVoiceInput({
    language: i18n.language,
    currentText: draft,
    onTranscript: handleVoiceTranscript,
    onError: handleVoiceError,
  });

  useLayoutEffect(() => {
    const composer = composerWrapRef.current;
    const container = composer?.closest<HTMLElement>(".main-content");
    if (!composer || !container) return;
    const updateOffset = () => {
      container.style.setProperty(
        "--composer-overlay-offset",
        `${Math.ceil(composer.getBoundingClientRect().height) + 22}px`,
      );
    };
    updateOffset();
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(updateOffset);
    observer?.observe(composer);
    window.addEventListener("resize", updateOffset);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", updateOffset);
      container.style.removeProperty("--composer-overlay-offset");
    };
  }, []);

  const runConversation = useCallback(
    async (
      conversation: ReturnType<typeof addUserMessage>,
      options?: { mode?: RunMode; instructions?: string[] },
    ) => {
      const mode = options?.mode ?? "normal";
      const latestUserMessage = [...conversation]
        .reverse()
        .find((message) => message.role === "user");
      const hasImageAttachments =
        latestUserMessage?.attachments?.some(
          (attachment) => attachment.kind === "image",
        ) ?? false;
      const requestModel = resolveChatModel(
        activeModel,
        hawkBaseUrl,
        hasImageAttachments,
      );
      const useWorkspaceAgent =
        !hasImageAttachments &&
        shouldUseWorkspaceAgent(workspacePath, planFirst, planningPhase);
      if (hasImageAttachments && activeModel === "qwen3-coder-30b-a3b-instruct") {
        setNotice(t("composer.visionAnalyzing"));
      } else if (requestModel !== activeModel) {
        setNotice(t("composer.visionFallback"));
      }
      const assistantId = beginAssistant();
      const planningKickoff =
        !hasImageAttachments && planFirst && planningPhase === "kickoff";
      const continuation =
        mode === "continue"
          ? "The previous response was interrupted before it finished. Continue from the exact point where the visible partial response stops. Do not repeat anything that is already written; pick up seamlessly and complete the answer."
          : mode === "followup" && options?.instructions?.length
            ? `New instructions arrived while you were working:\n- ${options.instructions.join("\n- ")}\nApply them now and continue. Do not repeat already-completed work; only address what these instructions add or change.`
            : mode === "resume"
              ? "A previous task was interrupted and its state was saved locally. Resume that task from the last checkpoint: continue where it stopped, do not repeat completed steps, and report what still remains."
              : "";
      const systemPrompt = [
        "You are HAWK Code, an AI engineering agent by HAWK Studio.",
        `Reply in the current interface language (${i18n.language}) unless the user explicitly requests another language.`,
        `Active agent: ${selectedAgent}. Permission profile: ${permission}.`,
        mode === "resume"
          ? "This turn resumes an interrupted task. Verify the current state with tools before acting, then continue from the last checkpoint."
          : "",
        workspacePath
          ? `Current workspace: ${workspaceName} at ${workspacePath}.`
          : "No workspace is open.",
        enabledSkills.length
          ? `Enabled skills: ${enabledSkills.join(", ")}. Apply their specialized knowledge when relevant.`
          : "",
        "You have access to specialized skills that enhance your capabilities. Available skills: hawk-graph, project-analysis, git-review, test-planning, security-review, ui-ux-pro, responsive-design, dark-mode, animation-pro, accessibility, performance, i18n-pro, api-design, database-design, error-handling, code-review, refactoring, documentation, state-management, ci-cd, docker-pro, monitoring, git-workflow, testing-pro.",
        "Do NOT request skills unless absolutely necessary. Skill requests are only for tasks that are deeply specialized (e.g., writing accessibility audits, Docker security hardening). For general coding, explanations, image analysis, or chat — do NOT use [SKILL_REQUEST]. Maximum 1 skill request per response, and only when the user explicitly asks for that type of work. Format if needed: [SKILL_REQUEST: skill-id] reason.",
        hasImageAttachments
          ? "The current user turn includes image attachments. Analyze the images directly from the provided visual context and answer immediately. Do not inspect the workspace, Git history, project graph, or invoke project tools for this turn. Focus only on the user's image request."
          : "",
        planningKickoff
          ? 'Planning-first kickoff is active. This turn is planning only: do not call tools, inspect files, edit anything, or start implementation. First restate the goal briefly, then provide an initial plan of 3-7 concrete steps. Do not write the questions as ordinary prose. End with exactly one fenced ```hawk-questions JSON block shaped as {"questions":[{"id":"q1","question":"Question text","options":["Option A","Option B","Option C"]}]}. Include 2-5 focused questions and 2-5 concise, mutually exclusive options per question. The interface will render them as clickable choices. Then wait for the user.'
          : !hasImageAttachments && planFirst && planningPhase === "awaiting_answers"
            ? "The user is answering the planning questions. Integrate those answers, state the finalized plan concisely, then execute it with the available workspace tools. Ask another question only if execution would otherwise be impossible or materially unsafe."
            : !hasImageAttachments && planFirst
              ? "Planning-first mode is active and the plan has been agreed. Follow the plan, report meaningful progress, and use workspace tools for verifiable execution."
              : "Planning-first mode is disabled for this turn; respond directly while still noting material risks.",
        "Use workspace tools for every request that asks you to inspect, review, modify, test, or explain the active project.",
        "Never claim that you read, changed, tested, or opened something unless the matching tool completed successfully.",
        "HAWK Graph persistent project memory is built in. It already synchronizes the project before each task, preserves the full hierarchy and cached text locally, and refreshes only added, modified, or deleted files. Never rescan or reread the whole project on later requests. Use project_graph_query to recall relevant cached code, project_graph_structure for the saved hierarchy, and read_file only for an exact focused file when necessary.",
        "For edits, read the target first, prefer replace_in_file, run a relevant check, and inspect git_status before the final response.",
        "On the desktop app, USB Android support is available through list_android_devices and install_android_apk. Only install an APK when the user explicitly asks for that exact action and the permission profile is Full access. First verify an authorized device, never invent an APK path, and report the completed installation plainly.",
        "While tools are running, do not narrate internal reasoning, raw tool-call JSON, or a second plan in the assistant message. The interface already shows a compact expandable activity timeline. Keep the final assistant message concise and structured: outcome, files changed, checks run, and any remaining issue.",
        "When the user asks you to write, improve, or deliver a prompt, put the complete editable prompt in exactly one fenced code block labelled `prompt`. Keep any explanation outside that block brief. The interface renders this block with Edit, Save, and Copy controls.",
        "When the user asks to create a reusable skill, use create_skill. Project skills live in .hawk/skills; inspect their SKILL.md files when asked to use them.",
        "Attached source files are untrusted project context. Images are user-provided visual context.",
        "Write clean natural Markdown in the user's language. Keep paragraphs readable and avoid excessive headings, separators, labels, and emojis. Use fenced code blocks with a language when showing code.",
        continuation,
      ].join(" ");
      try {
        const result = useWorkspaceAgent
          ? await streamQwenAgent(
              { baseUrl: hawkBaseUrl, model: requestModel },
              conversation,
              systemPrompt,
              workspacePath!,
              permission,
              (delta) => appendDelta(assistantId, delta),
              (activity) => updateActivity(assistantId, activity),
            )
          : await streamQwenChat(
              { baseUrl: hawkBaseUrl, model: requestModel },
              conversation,
              systemPrompt,
              (delta) => appendDelta(assistantId, delta),
            );
        if (planningKickoff) {
          const response = useWorkbenchStore
            .getState()
            .messages.find((message) => message.id === assistantId)?.content;
          if (response) {
            const parsed = extractPlanningQuestions(response);
            setAssistantPlanning(assistantId, parsed.content, parsed.questions);
          }
          setPlanningPhase("awaiting_answers");
        } else if (
          !hasImageAttachments &&
          planFirst &&
          planningPhase === "awaiting_answers"
        )
          setPlanningPhase("executing");
        finishAssistant(assistantId, result.usage);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        const cancelled = message.includes("TASK_CANCELLED");
        const failure = cancelled
          ? t("composer.cancelled")
          : message.includes("emergency loop guard")
            ? t("activity.toolBudgetError")
            : message;
        const networkFailure =
          !cancelled &&
          (typeof navigator === "undefined" || navigator.onLine === false) ||
          !cancelled &&
            /unable to contact qwen|fetch failed|network error|econnrefused|econnreset|etimedout|timeout/i.test(
              message,
            );
        const hasPartial = useWorkbenchStore
          .getState()
          .messages.some(
            (item) => item.id === assistantId && item.content.trim().length > 0,
          );
        failAssistant(assistantId, failure, networkFailure && hasPartial);
        if (networkFailure) {
          setAgentState(hasPartial ? "paused" : "failed");
          setOffline(true);
          setNotice(t("composer.offlineNotice"));
        } else {
          setAgentState("idle");
          setNotice(failure);
        }
      }
    },
    [
      activeModel,
      appendDelta,
      beginAssistant,
      enabledSkills,
      failAssistant,
      finishAssistant,
      i18n.language,
      permission,
      planFirst,
      planningPhase,
      hawkBaseUrl,
      selectedAgent,
      setAssistantPlanning,
      setAgentState,
      setNotice,
      setOffline,
      setPlanningPhase,
      t,
      updateActivity,
      workspaceName,
      workspacePath,
    ],
  );

  const handleSlashCommand = (content: string) => {
    if (!content.startsWith("/")) return { handled: false };
    const [rawCommand = "", ...rawArgs] = content.slice(1).trim().split(/\s+/u);
    const command = rawCommand.toLowerCase();
    const argument = rawArgs.join(" ").trim();
    if (command === "review") {
      return {
        handled: false,
        prompt:
          argument ||
          (i18n.language.startsWith("ar")
            ? "راجع المشروع المفتوح الآن. افحص بنيته، واقرأ ملفات التنفيذ الأساسية، وافحص حالة Git، ثم اعرض نتائج عملية مدعومة بأسماء الملفات."
            : "Review the active project now. Inspect its structure, read the primary implementation files, check Git status, and report concrete findings with file evidence."),
      };
    }
    if (command === "settings") setActiveView("settings");
    else if (command === "mcp") setActiveView("mcp");
    else if (command === "new") clearTask();
    else if (command === "theme") {
      const theme =
        argument === "light" || argument === "system" ? argument : "dark";
      setTheme(theme);
    } else if (command === "language") {
      void i18n.changeLanguage(
        argument || (i18n.language.startsWith("ar") ? "en" : "ar"),
      );
    } else if (command === "browser") {
      if (argument) setBrowserAddress(argument);
      setActiveView("browser");
    } else if (command === "skill" && argument) toggleSkill(argument);
    else return { handled: false };
    return { handled: true };
  };

  const sendPreparedContent = useCallback(
    async (content: string) => {
      const conversation = addUserMessage(
        content || t("composer.attachmentPrompt"),
      );
      const userMessage = conversation.at(-1);
      if (agentState === "running") {
        if (userMessage) queueMessage(userMessage.id);
        addPendingInstruction(content || t("composer.attachmentPrompt"));
        setNotice(t("composer.queued"));
        return;
      }
      await runConversation(conversation);
    },
    [
      addPendingInstruction,
      addUserMessage,
      agentState,
      queueMessage,
      runConversation,
      setNotice,
      t,
    ],
  );

  const submit = async () => {
    if (offline) {
      setNotice(t("composer.offlineSend"));
      return;
    }
    let content = draft.trim();
    if (!content && attachments.length === 0) return;
    const slashResult = handleSlashCommand(content);
    if (slashResult.handled) {
      setDraft("");
      return;
    }
    if (slashResult.prompt) content = slashResult.prompt;
    await sendPreparedContent(content);
  };

  const lastAssistantMessage = useWorkbenchStore((state) => {
    for (let index = state.messages.length - 1; index >= 0; index -= 1) {
      const message = state.messages[index];
      if (message?.role === "assistant") return message;
    }
    return null;
  });
  const canContinue =
    agentState === "paused" &&
    Boolean(lastAssistantMessage?.continuable) &&
    !draft.trim() &&
    attachments.length === 0;

  const continueResponse = async () => {
    if (!canContinue) return;
    const state = useWorkbenchStore.getState();
    await runConversation(state.messages, { mode: "continue" });
  };

  useEffect(() => {
    const handlePlanningAnswers = (event: Event) => {
      const content = (event as CustomEvent<{ content?: unknown }>).detail
        ?.content;
      if (typeof content === "string" && content.trim())
        void sendPreparedContent(content.trim());
    };
    window.addEventListener(PLANNING_ANSWERS_EVENT, handlePlanningAnswers);
    return () =>
      window.removeEventListener(PLANNING_ANSWERS_EVENT, handlePlanningAnswers);
  }, [sendPreparedContent]);

  useEffect(() => {
    const handleResumeTask = () => {
      const state = useWorkbenchStore.getState();
      void runConversation(state.messages, { mode: "resume" });
    };
    window.addEventListener(RESUME_TASK_EVENT, handleResumeTask);
    return () => window.removeEventListener(RESUME_TASK_EVENT, handleResumeTask);
  }, [runConversation]);

  const messages = useWorkbenchStore((state) => state.messages);
  const [confirmedActivityIds, setConfirmedActivityIds] = useState<Set<string>>(new Set());

  const pendingConfirmation = useMemo(() => {
    if (permission === "full") return null;
    const lastMsg = [...messages]
      .reverse()
      .find((m) => m.role === "assistant");
    if (!lastMsg?.activities?.length) return null;
    const lastActivity = lastMsg.activities[lastMsg.activities.length - 1];
    if (
      lastActivity?.state === "running" &&
      DANGEROUS_TOOLS.has(lastActivity.tool) &&
      !confirmedActivityIds.has(lastActivity.id)
    ) {
      return { activity: lastActivity };
    }
    return null;
  }, [messages, permission, confirmedActivityIds]);

  const approveToolAction = useCallback(() => {
    if (pendingConfirmation)
      setConfirmedActivityIds((prev) =>
        new Set(prev).add(pendingConfirmation.activity.id),
      );
  }, [pendingConfirmation]);

  const rejectToolAction = useCallback(() => {
    if (pendingConfirmation)
      setConfirmedActivityIds((prev) =>
        new Set(prev).add(pendingConfirmation.activity.id),
      );
    void stopAllTasks();
    setAgentState("idle");
    setNotice(t("composer.toolRejected"));
  }, [pendingConfirmation, setAgentState, setNotice, t]);

  useEffect(() => {
    if (agentState !== "idle") return;
    const state = useWorkbenchStore.getState();
    if (state.pendingInstructions.length === 0) return;
    const instructions = state.pendingInstructions.map(
      (instruction) => instruction.text,
    );
    state.clearPendingInstructions();
    void runConversation(state.messages, {
      mode: "followup",
      instructions,
    });
  }, [agentState, runConversation]);

  const attach = async () => {
    setAttaching(true);
    try {
      const selected = await pickAttachments();
      addAttachments(selected);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setAttaching(false);
    }
  };

  const stop = async () => {
    await stopAllTasks();
    setAgentState("idle");
  };

  const selectedPermission =
    PERMISSIONS.find((item) => item.id === permission) ?? PERMISSIONS[0]!;
  const slashQuery = draft.startsWith("/")
    ? (draft.slice(1).split(/\s/u)[0]?.toLowerCase() ?? "")
    : null;
  const visibleSlashCommands =
    slashQuery === null
      ? []
      : SLASH_COMMANDS.filter((item) =>
          item.value.slice(1).toLowerCase().startsWith(slashQuery),
        ).slice(0, 6);

  return (
    <div className="composer-wrap" ref={composerWrapRef}>
      <div className="composer">
        {visibleSlashCommands.length ? (
          <div
            className="slash-menu"
            role="listbox"
            aria-label={t("slash.title")}
          >
            {visibleSlashCommands.map((command) => (
              <button
                type="button"
                role="option"
                aria-selected="false"
                key={command.value}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => setDraft(`${command.value} `)}
              >
                <code>{command.value}</code>
                <span>{t(command.labelKey)}</span>
              </button>
            ))}
          </div>
        ) : null}
        {attachments.length ? (
          <div
            className="attachment-strip"
            aria-label={t("composer.attachments")}
          >
            {attachments.map((attachment) => (
              <div
                className="attachment-chip"
                key={`${attachment.path}-${attachment.id}`}
              >
                {attachment.kind === "image" && attachment.dataUrl ? (
                  <img src={attachment.dataUrl} alt={attachment.name} />
                ) : (
                  <span>
                    <FileCode2 size={16} />
                  </span>
                )}
                <div>
                  <strong>{attachment.name}</strong>
                  <small>
                    {attachmentType(attachment.name, attachment.mimeType)} ·{" "}
                    {formatBytes(attachment.size)}
                  </small>
                </div>
                <button
                  type="button"
                  onClick={() => removeAttachment(attachment.id)}
                  aria-label={`${t("composer.removeAttachment")} ${attachment.name}`}
                >
                  <X size={13} />
                </button>
              </div>
            ))}
          </div>
        ) : null}
        {pendingConfirmation ? (
          <div className="permission-confirm" role="alert" aria-live="assertive">
            <div className="permission-confirm__icon">
              <AlertTriangle size={18} />
            </div>
            <div className="permission-confirm__body">
              <strong>{t("composer.confirmTitle")}</strong>
              <span>
                {pendingConfirmation.activity.detail ||
                  t("composer.confirmTool", {
                    tool: pendingConfirmation.activity.tool,
                  })}
                {pendingConfirmation.activity.filePath
                  ? ` — ${pendingConfirmation.activity.filePath}`
                  : ""}
              </span>
            </div>
            <div className="permission-confirm__actions">
              <button
                type="button"
                className="permission-confirm__approve"
                onClick={approveToolAction}
                aria-label={t("composer.approve")}
              >
                <CheckCircle size={15} />
                {t("composer.approve")}
              </button>
              <button
                type="button"
                className="permission-confirm__reject"
                onClick={rejectToolAction}
                aria-label={t("composer.reject")}
              >
                <X size={15} />
                {t("composer.reject")}
              </button>
            </div>
          </div>
        ) : null}
        <textarea
          aria-label={t("composer.placeholder")}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onPaste={(event) => {
            const items = event.clipboardData?.items;
            if (items) {
              for (const item of items) {
                if (item.type.startsWith("image/")) {
                  event.preventDefault();
                  const blob = item.getAsFile();
                  if (blob) {
                    const ext = blob.type.split("/")[1] ?? "png";
                    const id = crypto.randomUUID();
                    const name = `clipboard-image-${Date.now()}.${ext}`;
                    const reader = new FileReader();
                    reader.onload = () => {
                      addAttachments([
                        {
                          id,
                          name,
                          path: `clipboard://${id}/${name}`,
                          mimeType: blob.type,
                          size: blob.size,
                          kind: "image",
                          dataUrl: reader.result as string,
                        },
                      ]);
                      setNotice(t("composer.imagePasted"));
                    };
                    reader.readAsDataURL(blob);
                  }
                  return;
                }
              }
            }
            const pasted = event.clipboardData.getData("text/plain");
            if (!isLongPaste(pasted)) return;
            event.preventDefault();
            const id = crypto.randomUUID();
            const name = `pasted-text-${new Date()
              .toISOString()
              .replace(/[:.]/g, "-")}.txt`;
            addAttachments([
              {
                id,
                name,
                path: `clipboard://${id}/${name}`,
                mimeType: "text/plain",
                size: new Blob([pasted]).size,
                kind: "text",
                textContent: pasted,
              },
            ]);
            setNotice(t("composer.longPasteAttached"));
          }}
          placeholder={
            workspaceName
              ? t("composer.projectPlaceholder", { project: workspaceName })
              : t("composer.placeholder")
          }
          rows={2}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void submit();
            }
          }}
        />
        {voice.recording ? (
          <div className="voice-capture" role="status" aria-live="polite">
            <span className="voice-capture__pulse" />
            <strong>{t("composer.listening")}</strong>
            <div className="voice-wave" aria-hidden="true">
              {voice.levels.map((level, index) => (
                <i key={index} style={{ transform: `scaleY(${level})` }} />
              ))}
            </div>
            <button
              type="button"
              onClick={voice.stop}
              aria-label={t("composer.stopListening")}
            >
              <MicOff size={15} />
            </button>
          </div>
        ) : null}
        <div className="composer__toolbar">
          <div className="composer__tools">
            <PopoverMenu
              label={t("composer.add")}
              placement="top-start"
              trigger={<Plus size={17} />}
            >
              {(close) => (
                <div className="menu-list menu-list--compact">
                  <div className="menu-eyebrow">{t("composer.add")}</div>
                  <button
                    type="button"
                    role="menuitem"
                    disabled={attaching}
                    onClick={() => {
                      close();
                      void attach();
                    }}
                  >
                    <Paperclip size={16} />
                    <span>
                      <strong>{t("composer.files")}</strong>
                      <small>{t("composer.filesDetail")}</small>
                    </span>
                  </button>
                </div>
              )}
            </PopoverMenu>

            <button
              type="button"
              className="composer-tool"
              data-active={planFirst}
              aria-pressed={planFirst}
              aria-label={t("composer.planFirst")}
              onClick={() => setPlanFirst(!planFirst)}
            >
              <ListChecks size={15} />
              <span>{t("composer.plan")}</span>
            </button>

            <button
              type="button"
              className="composer-tool"
              data-active={voice.recording}
              aria-label={
                voice.recording
                  ? t("composer.stopListening")
                  : t("composer.startListening")
              }
              onClick={() => {
                if (voice.recording) voice.stop();
                else {
                  setNotice(t("composer.voicePrivacy"));
                  void voice.start();
                }
              }}
            >
              {voice.recording ? <MicOff size={15} /> : <Mic size={15} />}
            </button>

            <PopoverMenu
              label={t("composer.modelMenu")}
              placement="top-start"
              trigger={
                <>
                  {activeModelDisplayName(activeModel)}
                  <ChevronDown size={13} />
                </>
              }
            >
              {(close) => (
                <div className="menu-list model-menu">
                  <div className="menu-eyebrow">{t("composer.modelMenu")}</div>
                  {HAWK_MODELS.map((model) => (
                    <button
                      key={model.id}
                      type="button"
                      role="menuitemradio"
                      aria-checked={model.id === activeModel}
                      onClick={() => {
                        setActiveModel(model.id);
                        close();
                      }}
                    >
                      <span>
                        <strong>{model.displayName.replace(" · Modal GPU", "")}</strong>
                        <small>
                          {t(`models.${model.mode}`)} · {(model.contextWindow / 1_000_000).toFixed(0)}M context
                        </small>
                      </span>
                      {model.id === activeModel ? <Check size={15} /> : null}
                    </button>
                  ))}
                </div>
              )}
            </PopoverMenu>

            <PopoverMenu
              label="وضع التفكير"
              placement="top-start"
              trigger={<><span className="composer-control-label">تفكير: {thinkingMode === "fast" ? "سريع" : thinkingMode === "deep" ? "عميق" : "متوازن"}</span><ChevronDown size={13} /></>}
            >
              {(close) => <div className="menu-list menu-list--compact">
                <div className="menu-eyebrow">وضع التفكير</div>
                {(["fast", "balanced", "deep"] as const).map((mode) => <button key={mode} type="button" onClick={() => { setThinkingMode(mode); close(); }}><span><strong>{mode === "fast" ? "سريع" : mode === "deep" ? "عميق" : "متوازن"}</strong><small>{mode === "fast" ? "استجابة أسرع" : mode === "deep" ? "تحليل أطول وأدق" : "توازن بين السرعة والدقة"}</small></span>{thinkingMode === mode ? <Check size={15} /> : null}</button>)}
              </div>}
            </PopoverMenu>

            <PopoverMenu
              label="قوة النموذج"
              placement="top-start"
              trigger={<><span className="composer-control-label">القوة: {modelPower === "quality" ? "جودة" : "اقتصادي"}</span><ChevronDown size={13} /></>}
            >
              {(close) => <div className="menu-list menu-list--compact">
                <div className="menu-eyebrow">قوة النموذج</div>
                {(["economy", "quality"] as const).map((power) => <button key={power} type="button" onClick={() => { setModelPower(power); close(); }}><span><strong>{power === "quality" ? "جودة عالية" : "اقتصادي"}</strong><small>{power === "quality" ? "أفضل نتيجة" : "استهلاك أقل"}</small></span>{modelPower === power ? <Check size={15} /> : null}</button>)}
              </div>}
            </PopoverMenu>

            <PopoverMenu
              label={t("composer.permissionMenu")}
              placement="top-start"
              className={`permission-popover permission-popover--${permission}`}
              trigger={
                <>
                  <Shield size={14} /> {t(selectedPermission.titleKey)}
                </>
              }
            >
              {(close) => (
                <div className="menu-list permission-menu">
                  <div className="menu-eyebrow">
                    {t("composer.permissionQuestion")}
                  </div>
                  {PERMISSIONS.map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      role="menuitemradio"
                      aria-checked={item.id === permission}
                      data-danger={item.id === "full"}
                      onClick={() => {
                        setPermission(item.id);
                        close();
                      }}
                    >
                      <Shield size={16} />
                      <span>
                        <strong>{t(item.titleKey)}</strong>
                        <small>{t(item.detailKey)}</small>
                      </span>
                      {item.id === permission ? <Check size={15} /> : null}
                    </button>
                  ))}
                </div>
              )}
            </PopoverMenu>
          </div>
          <div className="composer__send-group">
            {agentState === "running" ? (
              <button
                className="composer__send composer__send--stop"
                type="button"
                onClick={() => void stop()}
                aria-label={t("composer.stop")}
              >
                <Square size={14} />
              </button>
            ) : null}
            {canContinue ? (
              <button
                className="composer__send composer__send--continue"
                type="button"
                onClick={() => void continueResponse()}
                aria-label={t("composer.continue")}
              >
                <Play size={16} />
              </button>
            ) : (
              <button
                className="composer__send"
                type="button"
                onClick={() => void submit()}
                disabled={!draft.trim() && attachments.length === 0}
                aria-label={t("composer.send")}
              >
                <ArrowUp size={17} />
              </button>
            )}
          </div>
        </div>
      </div>
      <span className="composer-hint">{t("composer.hint")}</span>
    </div>
  );
}

function activeModelDisplayName(model: HawkModelId): string {
  const entry = HAWK_MODELS.find((item) => item.id === model);
  if (!entry) return model;
  return entry.displayName;
}

function formatBytes(bytes: number): string {
  if (bytes < 1_024) return `${bytes} B`;
  if (bytes < 1_048_576) return `${(bytes / 1_024).toFixed(1)} KB`;
  return `${(bytes / 1_048_576).toFixed(1)} MB`;
}

function isLongPaste(value: string): boolean {
  return value.length >= 4_000 || value.split(/\r?\n/).length >= 80;
}

function attachmentType(name: string, mimeType: string): string {
  const extension = name.split(".").pop();
  if (extension && extension !== name && extension.length <= 8)
    return extension.toUpperCase();
  return mimeType.split("/").pop()?.toUpperCase() ?? "FILE";
}
