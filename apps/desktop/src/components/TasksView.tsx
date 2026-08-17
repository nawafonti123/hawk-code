import {
  AlertCircle,
  ArrowDown,
  ArrowDownWideNarrow,
  ArrowUpWideNarrow,
  Check,
  CheckCircle,
  ChevronDown,
  Clock3,
  Copy,
  FileCode2,
  FileText,
  FolderSearch2,
  GitCompare,
  ListTodo,
  LoaderCircle,
  Pencil,
  TerminalSquare,
  Trash2,
  WifiOff,
  X,
  Zap,
} from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  AgentActivity,
  ChatMessage,
  PlanningQuestion,
} from "@hawk-code/shared-types";
import { getWorkspaceGitStatus } from "../lib/ipc";
import { formatPlanningAnswers, PLANNING_ANSWERS_EVENT } from "../lib/planning";
import {
  currentAgentStage,
  reasoningStageIndex,
  REASONING_STAGES,
} from "../lib/agent-stages";
import {
  clearCheckpoint,
  loadCheckpoint,
  RESUME_TASK_EVENT,
} from "../lib/task-checkpoints";
import {
  useWorkbenchStore,
  type PendingInstruction,
} from "../store/workbench";
import { ChangeSummaryCard } from "./ChangeSummaryCard";
import { MessageResponse } from "./MessageResponse";
import { WelcomeWorkbench } from "./WelcomeWorkbench";
import {
  type SkillRequest,
} from "../lib/skill-request";

export function TasksView() {
  const { t } = useTranslation();
  const messages = useWorkbenchStore((state) => state.messages);
  const queuedMessageIds = useWorkbenchStore((state) => state.queuedMessageIds);
  const agentState = useWorkbenchStore((state) => state.agentState);
  const pendingInstructions = useWorkbenchStore(
    (state) => state.pendingInstructions,
  );
  const offline = useWorkbenchStore((state) => state.offline);
  const workspacePath = useWorkbenchStore((state) => state.workspacePath);
  const conversationId = useWorkbenchStore((state) => state.conversationId);
  const conversationRef = useRef<HTMLElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const followingRef = useRef(true);
  const lastScrollTopRef = useRef(0);
  const streamFrameRef = useRef<number | null>(null);
  const [showJumpToBottom, setShowJumpToBottom] = useState(false);
  const activeAssistantId = [...messages]
    .reverse()
    .find((message) => message.role === "assistant")?.id;
  const checkpoint = loadCheckpoint();
  const showResume =
    Boolean(checkpoint) &&
    checkpoint!.status === "interrupted" &&
    checkpoint!.conversationId === conversationId &&
    agentState !== "running";
  const statusQuery = useQuery({
    queryKey: [
      "conversation-git-status",
      workspacePath,
      agentState,
      messages.length,
    ],
    queryFn: () => getWorkspaceGitStatus(workspacePath ?? ""),
    enabled:
      Boolean(workspacePath) &&
      messages.some((message) => message.role === "assistant") &&
      agentState !== "running",
    retry: false,
  });

  useEffect(() => {
    const conversation = conversationRef.current;
    if (!conversation || !followingRef.current) return;
    // Streaming can produce many short deltas in the same frame. A smooth
    // scroll for each one queues competing animations and causes visible jumps.
    if (streamFrameRef.current !== null) return;
    streamFrameRef.current = window.requestAnimationFrame(() => {
      conversation.scrollTop = conversation.scrollHeight;
      streamFrameRef.current = null;
    });
  }, [agentState, messages]);

  useEffect(() => {
    return () => {
      if (streamFrameRef.current !== null) {
        window.cancelAnimationFrame(streamFrameRef.current);
        streamFrameRef.current = null;
      }
    };
  }, []);

  if (messages.length === 0) return <WelcomeWorkbench />;

  const followLatest = () => {
    followingRef.current = true;
    setShowJumpToBottom(false);
    const conversation = conversationRef.current;
    conversation?.scrollTo?.({
      top: conversation.scrollHeight,
      behavior: "smooth",
    });
  };

  return (
    <>
      <main
        ref={conversationRef}
        className="conversation"
        aria-label="HAWK conversation"
        onScroll={(event) => {
          const conversation = event.currentTarget;
          const distanceFromBottom =
            conversation.scrollHeight -
            conversation.scrollTop -
            conversation.clientHeight;
          const nearBottom = distanceFromBottom <= 72;
          const movedUp = conversation.scrollTop < lastScrollTopRef.current - 2;
          if (nearBottom) {
            followingRef.current = true;
            setShowJumpToBottom(false);
          } else if (movedUp || !followingRef.current) {
            followingRef.current = false;
            setShowJumpToBottom(true);
          }
          lastScrollTopRef.current = conversation.scrollTop;
        }}
      >
        <div className="conversation__stream">
          {showResume && checkpoint ? (
            <ResumeBanner checkpoint={checkpoint} />
          ) : null}
          {offline && messages.length > 0 ? <OfflineBanner /> : null}
          {messages.map((message) => (
            <ConversationMessage
              key={message.id}
              message={message}
              queued={queuedMessageIds.includes(message.id)}
              running={
                message.role === "assistant" &&
                agentState === "running" &&
                message.id === activeAssistantId
              }
            />
          ))}
          {agentState === "running" && pendingInstructions.length > 0 ? (
            <PendingInstructionsCard instructions={pendingInstructions} />
          ) : null}
          {workspacePath && statusQuery.data ? (
            <ChangeSummaryCard
              status={statusQuery.data}
              workspacePath={workspacePath}
            />
          ) : null}
          <div ref={endRef} />
        </div>
      </main>
      {showJumpToBottom ? (
        <button
          type="button"
          className="conversation-jump"
          onClick={followLatest}
          aria-label={t("conversation.jumpToBottom")}
        >
          <ArrowDown size={15} />
          <span>{t("conversation.jumpToBottom")}</span>
        </button>
      ) : null}
    </>
  );
}

function ConversationMessage({
  message,
  queued,
  running,
}: {
  message: ChatMessage;
  queued: boolean;
  running: boolean;
}) {
  const { t } = useTranslation();
  const updateMessageContent = useWorkbenchStore(
    (state) => state.updateMessageContent,
  );
  const [copied, setCopied] = useState(false);
  const visibleContent =
    message.role === "assistant"
      ? cleanAssistantContent(message.content)
      : message.content;
  const copy = async () => {
    await navigator.clipboard.writeText(visibleContent);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1_600);
  };
  return (
    <article className={`message message--${message.role}`}>
      <div className="message__body">
        {message.attachments?.length ? (
          <div className="message-attachments">
            {message.attachments.map((attachment) =>
              attachment.kind === "image" && attachment.dataUrl ? (
                <img
                  key={`${attachment.path}-${attachment.id}`}
                  src={attachment.dataUrl}
                  alt={attachment.name}
                />
              ) : (
                <span key={`${attachment.path}-${attachment.id}`}>
                  <FileText size={14} />
                  {attachment.name}
                </span>
              ),
            )}
          </div>
        ) : null}
        {message.activities?.length ? (
          <AgentActivityList
            activities={message.activities}
            running={running}
            failed={Boolean(message.error)}
          />
        ) : null}
        {message.role === "assistant" &&
        (running || typeof message.durationMs === "number") ? (
          <ResponseTiming message={message} running={running} />
        ) : null}
        {visibleContent ? (
          <MessageResponse
            onPromptSave={(previous, next) =>
              updateMessageContent(
                message.id,
                replacePrompt(message.content, previous, next),
              )
            }
          >
            {visibleContent}
          </MessageResponse>
        ) : null}
        {message.role === "assistant" &&
        (() => {
          const skillRequests = extractSkillRequests(message.content);
          return skillRequests.length > 0 ? (
            <SkillRequestCards requests={skillRequests} />
          ) : null;
        })()}
        {message.planningQuestions?.length ? (
          <PlanningQuestionCards questions={message.planningQuestions} />
        ) : null}
        {running && !message.content ? (
          <DeepThinkingPanel message={message} running />
        ) : null}
        {message.error ? (
          <p className="message__error">
            <AlertCircle size={15} />
            {message.error === "The agent turn did not complete."
              ? t("activity.interrupted")
              : message.error}
          </p>
        ) : null}
        {queued ? (
          <span className="message__queued">{t("composer.queuedLabel")}</span>
        ) : null}
      </div>
      {message.role === "assistant" && visibleContent ? (
        <div className="message__actions">
          <button
            type="button"
            onClick={() => void copy()}
            aria-label={t("message.copy")}
          >
            {copied ? <Check size={14} /> : <Copy size={14} />}
            <span>{copied ? t("message.copied") : t("message.copy")}</span>
          </button>
        </div>
      ) : null}
    </article>
  );
}

/** Keep transport-level tool syntax out of the user-facing final response. */
function cleanAssistantContent(content: string): string {
  const withoutToolTags = content
    .replace(/<tool_call>[\s\S]*?<\/tool_call>/giu, "")
    .replace(/<analysis>[\s\S]*?<\/analysis>/giu, "");
  const withoutRawCalls = withoutToolTags.replace(
    /```(?:json)?\s*\[\s*\{\s*"id"\s*:\s*"call_[\s\S]*?\}\s*\]\s*```|\[\s*\{\s*"id"\s*:\s*"call_[\s\S]*?\}\s*\]/giu,
    "",
  );
  const withoutSkillRequests = withoutRawCalls.replace(
    /\[SKILL_REQUEST:\s*\S+\]\s*.*/gim,
    "",
  );
  return withoutSkillRequests.replace(/\n{3,}/gu, "\n\n").trim();
}

/**
 * Replaces the rendered prompt block inside the raw assistant content.
 * The block must match by its normalized inner text so saves survive the
 * cleaning pipeline (trailing blank lines, trimmed content, CRLF, and
 * collapsed runs of blank lines).
 */
function replacePrompt(content: string, previous: string, next: string): string {
  const promptFence = "```prompt";
  const normalizedPrevious = normalizeBlockText(previous);
  let cursor = 0;
  while (cursor < content.length) {
    const opening = content.indexOf(promptFence, cursor);
    if (opening < 0) return content;
    const innerStart = content.indexOf("\n", opening) + 1;
    if (innerStart <= 0) return content;
    const closing = content.indexOf("```", innerStart);
    if (closing < 0) return content;
    const inner = content.slice(innerStart, closing);
    if (normalizeBlockText(inner) === normalizedPrevious)
      return content.slice(0, innerStart) + next + content.slice(closing);
    cursor = closing + 3;
  }
  return content;
}

function normalizeBlockText(value: string): string {
  return value.replace(/\r\n?/gu, "\n").trim().replace(/\n{3,}/gu, "\n\n");
}

const SKILL_REQUEST_GLOBAL_RE = /\[SKILL_REQUEST:\s*(\S+)\]\s*(.*)/gim;

function extractSkillRequests(content: string): SkillRequest[] {
  const requests: SkillRequest[] = [];
  let match: RegExpExecArray | null;
  while ((match = SKILL_REQUEST_GLOBAL_RE.exec(content)) !== null) {
    const skillId = match[1]?.trim();
    const reason = match[2]?.trim() ?? "";
    if (skillId) requests.push({ skillId, reason });
  }
  return requests;
}

const SKILL_NAMES: Record<string, string> = {
  "hawk-graph": "HAWK Graph",
  "project-analysis": "Project Analysis",
  "git-review": "Git Review",
  "test-planning": "Test Planning",
  "security-review": "Security Review",
  "ui-ux-pro": "UI/UX Pro",
  "responsive-design": "Responsive Design",
  "animation-pro": "Animation Pro",
  "accessibility": "Accessibility",
  "performance": "Performance",
  "i18n-pro": "i18n Pro",
  "api-design": "API Design",
  "database-design": "Database Design",
  "error-handling": "Error Handling",
  "code-review": "Code Review",
  "refactoring": "Refactoring",
  "documentation": "Documentation",
  "state-management": "State Management",
  "ci-cd": "CI/CD",
  "docker-pro": "Docker Pro",
  "monitoring": "Monitoring",
  "git-workflow": "Git Workflow",
  "testing-pro": "Testing Pro",
};

function SkillRequestCards({
  requests,
}: {
  requests: SkillRequest[];
}) {
  const { t } = useTranslation();
  const enabledSkills = useWorkbenchStore((state) => state.enabledSkills);
  const approveSkill = useWorkbenchStore((state) => state.approveSkill);
  const declineSkill = useWorkbenchStore((state) => state.declineSkill);
  const [handled, setHandled] = useState<Record<string, "approved" | "declined">>({});
  const approve = (skillId: string) => {
    useWorkbenchStore.getState().requestSkill(skillId, "");
    approveSkill();
    setHandled((prev) => ({ ...prev, [skillId]: "approved" }));
  };
  const decline = (skillId: string) => {
    declineSkill();
    setHandled((prev) => ({ ...prev, [skillId]: "declined" }));
  };
  return (
    <div className="skill-request-cards" aria-label={t("composer.skillRequests")}>
      {requests.map((request) => {
        const status = handled[request.skillId];
        const alreadyEnabled = enabledSkills.includes(request.skillId);
        const displayName =
          SKILL_NAMES[request.skillId] ?? request.skillId;
        return (
          <div
            className="skill-request-card"
            key={request.skillId}
            data-status={status ?? (alreadyEnabled ? "approved" : "pending")}
          >
            <div className="skill-request-card__icon">
              <Zap size={15} />
            </div>
            <div className="skill-request-card__body">
              <strong>{displayName}</strong>
              {request.reason ? <span>{request.reason}</span> : null}
            </div>
            <div className="skill-request-card__actions">
              {status === "approved" || alreadyEnabled ? (
                <span className="skill-request-card__done">
                  <CheckCircle size={14} /> {t("skills.on")}
                </span>
              ) : status === "declined" ? (
                <span className="skill-request-card__done skill-request-card__done--declined">
                  {t("skills.off")}
                </span>
              ) : (
                <>
                  <button
                    type="button"
                    className="skill-request-card__approve"
                    onClick={() => approve(request.skillId)}
                    aria-label={t("composer.approve")}
                  >
                    <CheckCircle size={14} /> {t("composer.approve")}
                  </button>
                  <button
                    type="button"
                    className="skill-request-card__decline"
                    onClick={() => decline(request.skillId)}
                    aria-label={t("composer.reject")}
                  >
                    <X size={14} /> {t("composer.reject")}
                  </button>
                </>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function PlanningQuestionCards({
  questions,
}: {
  questions: PlanningQuestion[];
}) {
  const { t } = useTranslation();
  const [selections, setSelections] = useState<Record<string, string>>({});
  const [submitted, setSubmitted] = useState(false);
  const complete = questions.every((question) => selections[question.id]);
  const submitAnswers = () => {
    if (!complete || submitted) return;
    setSubmitted(true);
    window.dispatchEvent(
      new CustomEvent(PLANNING_ANSWERS_EVENT, {
        detail: { content: formatPlanningAnswers(questions, selections) },
      }),
    );
  };
  return (
    <section
      className="planning-questions"
      aria-label={t("planning.questionsTitle")}
    >
      <div className="planning-questions__header">
        <strong>{t("planning.questionsTitle")}</strong>
        <span>
          {t("planning.progress", {
            selected: Object.keys(selections).length,
            total: questions.length,
          })}
        </span>
      </div>
      {questions.map((question, questionIndex) => (
        <div className="planning-question" key={question.id}>
          <p>
            <span>{questionIndex + 1}</span>
            {question.question}
          </p>
          <div
            className="planning-question__options"
            role="radiogroup"
            aria-label={question.question}
          >
            {question.options.map((option) => {
              const selected = selections[question.id] === option;
              return (
                <button
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  data-selected={selected}
                  key={option}
                  onClick={() =>
                    setSelections((current) => ({
                      ...current,
                      [question.id]: option,
                    }))
                  }
                >
                  <span className="planning-option__indicator">
                    {selected ? <Check size={13} /> : null}
                  </span>
                  {option}
                </button>
              );
            })}
          </div>
        </div>
      ))}
      <button
        type="button"
        className="planning-questions__submit"
        disabled={!complete || submitted}
        onClick={submitAnswers}
      >
        {t("planning.submitAnswers")}
      </button>
    </section>
  );
}

function ResponseTiming({
  message,
  running,
}: {
  message: ChatMessage;
  running: boolean;
}) {
  const { i18n, t } = useTranslation();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    const interval = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(interval);
  }, [running]);
  const startedAt = Date.parse(message.createdAt);
  const elapsed = running
    ? Math.max(0, now - (Number.isFinite(startedAt) ? startedAt : now))
    : (message.durationMs ?? 0);
  const thinking = message.thinkingDurationMs;
  const label = running
    ? typeof thinking === "number"
      ? t("timing.responding", {
          time: formatDuration(elapsed, i18n.language),
        })
      : t("timing.thinking", {
          time: formatDuration(elapsed, i18n.language),
        })
    : t("timing.completed", {
        thinking: formatDuration(thinking ?? elapsed, i18n.language),
        total: formatDuration(elapsed, i18n.language),
      });
  return (
    <div className="response-timing" role={running ? "status" : undefined}>
      <Clock3 size={13} />
      <span>{label}</span>
    </div>
  );
}

function formatDuration(milliseconds: number, language: string): string {
  const seconds = Math.max(0, milliseconds) / 1_000;
  const formatter = new Intl.NumberFormat(language, {
    maximumFractionDigits: seconds < 10 ? 1 : 0,
    minimumFractionDigits: seconds < 10 ? 1 : 0,
  });
  if (seconds < 60)
    return `${formatter.format(seconds)} ${language.startsWith("ar") ? "ث" : "s"}`;
  const minutes = Math.floor(seconds / 60);
  const remaining = Math.round(seconds % 60);
  return `${minutes}:${remaining.toString().padStart(2, "0")}`;
}

function AgentActivityList({
  activities,
  running,
  failed,
}: {
  activities: AgentActivity[];
  running: boolean;
  failed: boolean;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const panelId = useId();
  const current = [...activities]
    .reverse()
    .find((activity) => activity.state === "running");
  const latest = current ?? activities.at(-1);
  const hasFailed =
    failed || activities.some((activity) => activity.state === "failed");
  const summary = running
    ? latest
      ? t(`activity.${latest.tool}`)
      : t("activity.working")
    : hasFailed
      ? t("activity.stoppedSteps", { count: activities.length })
      : t("activity.completedSteps", { count: activities.length });

  return (
    <div
      className="agent-activity"
      data-running={running}
      data-failed={hasFailed}
      aria-label={t("activity.title")}
    >
      <button
        type="button"
        className="agent-activity__summary"
        aria-expanded={expanded}
        aria-controls={panelId}
        onClick={() => setExpanded((value) => !value)}
      >
        <span className="agent-activity__summary-icon" aria-hidden="true">
          {running ? (
            <LoaderCircle className="spin" size={15} />
          ) : hasFailed ? (
            <AlertCircle size={15} />
          ) : (
            <Check size={15} />
          )}
        </span>
        <span
          className="agent-activity__summary-label"
          data-shimmer={running}
          role="status"
          aria-live="polite"
        >
          {summary}
        </span>
        <ChevronDown
          className="agent-activity__chevron"
          data-expanded={expanded}
          size={15}
          aria-hidden="true"
        />
      </button>
      {expanded ? (
        <div className="agent-activity__details" id={panelId}>
          {activities.map((activity) => {
            const Icon = activityIcon(activity.tool);
            return (
              <div
                className="agent-activity__row"
                data-state={activity.state}
                key={activity.id}
              >
                <span className="agent-activity__icon">
                  {activity.state === "running" ? (
                    <LoaderCircle className="spin" size={15} />
                  ) : activity.state === "failed" ? (
                    <AlertCircle size={15} />
                  ) : (
                    <Icon size={15} />
                  )}
                </span>
                <span>
                  <strong>{t(`activity.${activity.tool}`)}</strong>
                  <small>{activity.filePath ?? activity.detail}</small>
                </span>
                {activity.state === "completed" ? <Check size={14} /> : null}
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

function activityIcon(tool: string) {
  if (tool === "list_files" || tool === "project_graph_structure")
    return FolderSearch2;
  if (tool === "project_graph_query") return FileText;
  if (
    tool === "read_file" ||
    tool === "read_files" ||
    tool === "write_file" ||
    tool === "replace_in_file"
  )
    return FileCode2;
  if (tool === "git_status") return GitCompare;
  return TerminalSquare;
}

/** Live reasoning progress. Derived from real tool activity — raw
 * chain-of-thought is never shown. */
function DeepThinkingPanel({
  message,
  running,
}: {
  message: ChatMessage;
  running: boolean;
}) {
  const { t } = useTranslation();
  const planningPhase = useWorkbenchStore((state) => state.planningPhase);
  const planFirst = useWorkbenchStore((state) => state.planFirst);
  const stage = currentAgentStage({
    agentState: running ? "running" : "idle",
    activities: message.activities ?? [],
    hasContent: message.content.length > 0,
    planningPhase,
    planningKickoff: planFirst && planningPhase === "kickoff",
  });
  const current = [...(message.activities ?? [])]
    .reverse()
    .find((activity) => activity.state === "running");
  const detail = current?.filePath ?? current?.detail ?? null;
  return (
    <div
      className="deep-thinking"
      data-stage={stage}
      role="status"
      aria-live="polite"
    >
      <div className="deep-thinking__stages">
        {REASONING_STAGES.map((item) => {
          const index = REASONING_STAGES.indexOf(item);
          const state =
            stage === item
              ? "active"
              : reasoningStageIndex(stage) > index
                ? "done"
                : "pending";
          return (
            <span
              className="deep-thinking__stage"
              data-state={state}
              key={item}
            >
              {state === "done" ? (
                <Check size={12} />
              ) : (
                <span className="deep-thinking__dot" aria-hidden="true" />
              )}
              {t(`stages.${item}`)}
            </span>
          );
        })}
      </div>
      <div className="deep-thinking__detail" data-shimmer>
        {detail ?? t(`stages.${stage}`)}
      </div>
    </div>
  );
}

/** Instructions sent while the agent is working. They apply when the current
 * step finishes; the queue can be edited and reordered. */
function PendingInstructionsCard({
  instructions,
}: {
  instructions: PendingInstruction[];
}) {
  const { t } = useTranslation();
  const updateInstruction = useWorkbenchStore(
    (state) => state.updatePendingInstruction,
  );
  const removeInstruction = useWorkbenchStore(
    (state) => state.removePendingInstruction,
  );
  const moveInstruction = useWorkbenchStore(
    (state) => state.movePendingInstruction,
  );
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  return (
    <div className="pending-card">
      <header className="pending-card__header">
        <ListTodo size={15} />
        <strong>{t("pending.title")}</strong>
        <span className="pending-card__badge">{instructions.length}</span>
        <small>{t("pending.hint")}</small>
      </header>
      <ul className="pending-card__list">
        {instructions.map((instruction, index) => (
          <li className="pending-card__item" key={instruction.id}>
            {editingId === instruction.id ? (
              <div className="pending-card__editor">
                <textarea
                  value={draft}
                  aria-label={t("pending.edit")}
                  onChange={(event) => setDraft(event.target.value)}
                  rows={2}
                />
                <button
                  type="button"
                  onClick={() => {
                    updateInstruction(instruction.id, draft);
                    setEditingId(null);
                  }}
                >
                  {t("prompt.save")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setEditingId(null);
                    setDraft("");
                  }}
                >
                  {t("prompt.cancel")}
                </button>
              </div>
            ) : (
              <button
                type="button"
                className="pending-card__text"
                onClick={() => {
                  setEditingId(instruction.id);
                  setDraft(instruction.text);
                }}
              >
                {instruction.text}
              </button>
            )}
            <span className="pending-card__actions">
              <button
                type="button"
                aria-label={t("pending.moveUp")}
                disabled={index === 0}
                onClick={() => moveInstruction(instruction.id, "up")}
              >
                <ArrowUpWideNarrow size={14} />
              </button>
              <button
                type="button"
                aria-label={t("pending.moveDown")}
                disabled={index === instructions.length - 1}
                onClick={() => moveInstruction(instruction.id, "down")}
              >
                <ArrowDownWideNarrow size={14} />
              </button>
              <button
                type="button"
                aria-label={t("pending.edit")}
                onClick={() => {
                  setEditingId(instruction.id);
                  setDraft(instruction.text);
                }}
              >
                <Pencil size={14} />
              </button>
              <button
                type="button"
                aria-label={t("pending.delete")}
                onClick={() => removeInstruction(instruction.id)}
              >
                <Trash2 size={14} />
              </button>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Recovery banner shown after a restart when an interrupted task was saved. */
function ResumeBanner({
  checkpoint,
}: {
  checkpoint: NonNullable<ReturnType<typeof loadCheckpoint>>;
}) {
  const { i18n, t } = useTranslation();
  const setNotice = useWorkbenchStore((state) => state.setNotice);
  const [reviewing, setReviewing] = useState(false);
  const [age] = useState(() => {
    const lastActivity = new Date(checkpoint.lastActivityAt).getTime();
    return Math.max(0, Date.now() - lastActivity);
  });
  return (
    <section className="resume-banner" aria-label={t("resume.title")}>
      <div className="resume-banner__main">
        <span className="resume-banner__icon" aria-hidden="true">
          <Clock3 size={16} />
        </span>
        <div className="resume-banner__text">
          <strong>{t("resume.title")}</strong>
          <small>
            {t("resume.body", {
              task: checkpoint.title,
              steps: checkpoint.stepCount,
              time: formatRelativeAge(age, i18n.language),
            })}
          </small>
        </div>
        <div className="resume-banner__actions">
          <button
            type="button"
            className="resume-banner__primary"
            onClick={() =>
              window.dispatchEvent(new Event(RESUME_TASK_EVENT))
            }
          >
            {t("resume.continue")}
          </button>
          <button
            type="button"
            aria-expanded={reviewing}
            onClick={() => setReviewing((value) => !value)}
          >
            {t("resume.review")}
          </button>
          <button
            type="button"
            onClick={() => {
              clearCheckpoint();
              setNotice(t("resume.dismissed"));
            }}
          >
            {t("resume.cancel")}
          </button>
        </div>
      </div>
      {reviewing ? (
        <div className="resume-banner__details">
          <span>
            {t("resume.progress", { steps: checkpoint.stepCount })}
          </span>
          <span>
            {t("resume.files", {
              count: checkpoint.filesChanged.length,
            })}
          </span>
          {checkpoint.filesChanged.length ? (
            <code>{checkpoint.filesChanged.join(", ")}</code>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

function formatRelativeAge(milliseconds: number, language: string): string {
  const formatter = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
  const seconds = milliseconds / 1_000;
  if (seconds < 60) return formatter.format(0, "minute");
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return formatter.format(-minutes, "minute");
  return formatter.format(-Math.floor(minutes / 60), "hour");
}

/** Offline state: the task pauses and every state stays saved locally. */
function OfflineBanner() {
  const { t } = useTranslation();
  return (
    <div className="offline-banner" role="status" aria-live="polite">
      <WifiOff size={15} />
      <span>{t("offline.title")}</span>
      <small>{t("offline.body")}</small>
    </div>
  );
}
