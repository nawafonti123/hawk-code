import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "./i18n";
import { App } from "./App";
import { ChangeSummaryCard } from "./components/ChangeSummaryCard";
import { resolveChatModel } from "./lib/qwen-model";
import {
  extractPlanningQuestions,
  formatPlanningAnswers,
  shouldUseWorkspaceAgent,
} from "./lib/planning";
import { useWorkbenchStore } from "./store/workbench";

function renderApp() {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <App />
    </QueryClientProvider>,
  );
}

describe("HAWK Code workbench", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.localStorage.clear();
    useWorkbenchStore.setState({
      authenticated: true,
      activeView: "tasks",
      commandPaletteOpen: false,
      messages: [],
      attachments: [],
      composerDraft: "",
      agentState: "idle",
      permissionProfile: "ask",
      planFirst: false,
      planningPhase: "kickoff",
      theme: "system",
      pendingInstructions: [],
      offline: false,
    });
  });

  it("shows secure authentication before the workbench", () => {
    useWorkbenchStore.setState({ authenticated: false });
    renderApp();
    expect(
      screen.getByRole("heading", { name: /welcome back/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Sign in with Google/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Sign in with GitHub/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Sign in with Facebook/i }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/OAuth Client ID/i)).not.toBeInTheDocument();
  });

  it("renders the project-scoped workspace entry point", () => {
    renderApp();
    expect(screen.getByLabelText("HAWK Code")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /open local project/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("navigation", { name: /main navigation/i }),
    ).toBeInTheDocument();
  });

  it("opens the command palette from the keyboard", () => {
    renderApp();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(
      screen.getByRole("dialog", { name: /command palette/i }),
    ).toBeInTheDocument();
  });

  it("opens the real model and permission menus from the composer", () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Choose model" }));
    expect(
      screen.getByRole("menuitemradio", { name: /Hawk K3/i }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Choose permissions" }));
    expect(
      screen.getByRole("menuitemradio", { name: /Full access/i }),
    ).toBeInTheDocument();
  });

  it("offers Hawk K3 as the single real model with best defaults", () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Choose model" }));
    const options = screen.getAllByRole("menuitemradio");
    expect(options).toHaveLength(1);
      expect(options[0]).toHaveTextContent(/Hawk K3/i);
    expect(options[0]).toHaveAttribute("aria-checked", "true");
    expect(useWorkbenchStore.getState().activeModel).toBe(
      "qwen3-coder-30b-a3b-instruct",
    );
  });

  it("keeps the sidebar as one scrollable region with a fixed bottom bar", () => {
    renderApp();
    const scroll = document.querySelector(".sidebar__scroll");
    const bottom = document.querySelector(".sidebar__bottom");
    expect(scroll).not.toBeNull();
    expect(bottom).not.toBeNull();
    expect(bottom?.parentElement).toBe(scroll?.parentElement);
    const lists = document.querySelector(".sidebar-lists");
    expect(scroll?.contains(lists)).toBe(true);
  });

  it("renders composer menus in a body portal outside the clipped field", () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Choose model" }));
    expect(document.querySelector(".composer .popover-panel")).toBeNull();
    expect(
      document.body.querySelector(".popover-panel--portal"),
    ).toBeInTheDocument();
  });

  it("turns very long pasted text into a typed TXT attachment", () => {
    renderApp();
    const text = "long clipboard context\n".repeat(220);
    fireEvent.paste(
      screen.getByRole("textbox", { name: /Give HAWK a task/i }),
      {
        clipboardData: { getData: () => text },
      },
    );
    const [attachment] = useWorkbenchStore.getState().attachments;
    expect(attachment?.name).toMatch(/^pasted-text-.*\.txt$/);
    expect(attachment?.mimeType).toBe("text/plain");
    expect(attachment?.textContent).toBe(text);
    expect(screen.getByText(/TXT ·/i)).toBeInTheDocument();
  });

  it("keeps agent work collapsed behind a shimmering status control", () => {
    useWorkbenchStore.setState({
      agentState: "running",
      messages: [
        {
          id: "assistant-review",
          role: "assistant",
          content: "",
          createdAt: new Date().toISOString(),
          activities: [
            {
              id: "read-app",
              tool: "read_file",
              state: "running",
              detail: "Reading the application shell",
              filePath: "src/App.tsx",
            },
          ],
        },
      ],
    });
    renderApp();
    const status = screen.getByRole("button", { name: /Read file/i });
    expect(status).toHaveAttribute("aria-expanded", "false");
    expect(document.querySelector(".agent-activity__details")).toBeNull();
    fireEvent.click(status);
    expect(status).toHaveAttribute("aria-expanded", "true");
    expect(document.querySelector(".agent-activity__details")).not.toBeNull();
  });

  it("enables planning-first mode from the real composer control", () => {
    renderApp();
    const button = screen.getByRole("button", {
      name: /Plan first and ask focused questions/i,
    });
    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(button);
    expect(useWorkbenchStore.getState().planFirst).toBe(true);
    fireEvent.click(button);
    expect(useWorkbenchStore.getState().planFirst).toBe(false);
    expect(useWorkbenchStore.getState().planningPhase).toBe("executing");
  });

  it("keeps the first planning turn tool-free, then enables execution", () => {
    expect(shouldUseWorkspaceAgent("C:\\project", true, "kickoff")).toBe(false);
    expect(
      shouldUseWorkspaceAgent("C:\\project", true, "awaiting_answers"),
    ).toBe(true);
    expect(shouldUseWorkspaceAgent("C:\\project", false, "executing")).toBe(
      true,
    );
  });

  it("extracts structured planning choices and formats selected answers", () => {
    const parsed = extractPlanningQuestions(`خطة أولية

\`\`\`hawk-questions
{"questions":[{"id":"scope","question":"ما النطاق؟","options":["كامل","جزئي"]}]}
\`\`\``);
    expect(parsed.content).toBe("خطة أولية");
    expect(parsed.questions).toHaveLength(1);
    expect(
      formatPlanningAnswers(parsed.questions, { scope: "كامل" }),
    ).toContain("ما النطاق؟\nكامل");
  });

  it("renders planning questions as choices and sends the selected answers", () => {
    useWorkbenchStore.setState({
      agentState: "running",
      messages: [
        {
          id: "planning-message",
          role: "assistant",
          content: "Initial plan",
          createdAt: new Date().toISOString(),
          planningQuestions: [
            {
              id: "scope",
              question: "Choose the scope",
              options: ["Full project", "Core files"],
            },
          ],
        },
      ],
    });
    renderApp();
    fireEvent.click(screen.getByRole("radio", { name: "Full project" }));
    const submitAnswers = screen.getByRole("button", {
      name: "Continue with these answers",
    });
    expect(submitAnswers).toBeEnabled();
    fireEvent.click(submitAnswers);
    expect(useWorkbenchStore.getState().messages.at(-1)?.content).toContain(
      "Choose the scope\nFull project",
    );
  });

  it("records thinking time and total response duration", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-08T20:00:00Z"));
    const assistantId = useWorkbenchStore.getState().beginAssistantMessage();
    vi.advanceTimersByTime(1_200);
    useWorkbenchStore.getState().appendAssistantDelta(assistantId, "Hello");
    vi.advanceTimersByTime(800);
    useWorkbenchStore.getState().finishAssistantMessage(assistantId, {
      promptTokens: 0,
      completionTokens: 0,
      totalTokens: 0,
    });
    const message = useWorkbenchStore
      .getState()
      .messages.find((item) => item.id === assistantId);
    expect(message?.thinkingDurationMs).toBe(1_200);
    expect(message?.durationMs).toBe(2_000);
    vi.useRealTimers();
  });

  it("pauses chat following when scrolling up and offers a jump to latest", () => {
    useWorkbenchStore.setState({
      agentState: "idle",
      messages: [
        {
          id: "long-message",
          role: "assistant",
          content: "A long response",
          createdAt: new Date().toISOString(),
          durationMs: 500,
          thinkingDurationMs: 200,
        },
      ],
    });
    renderApp();
    const conversation = screen.getByLabelText("HAWK conversation");
    Object.defineProperties(conversation, {
      scrollHeight: { configurable: true, value: 1_000 },
      clientHeight: { configurable: true, value: 400 },
      scrollTop: { configurable: true, value: 500, writable: true },
    });
    const scrollTo = vi.fn();
    conversation.scrollTo = scrollTo;
    fireEvent.scroll(conversation);
    conversation.scrollTop = 250;
    fireEvent.scroll(conversation);
    const jump = screen.getByRole("button", { name: "Jump to latest" });
    fireEvent.click(jump);
    expect(scrollTo).toHaveBeenCalledWith({
      top: 1_000,
      behavior: "smooth",
    });
  });

  it("keeps the account menu focused and removes setup shortcuts", () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Account menu" }));
    expect(
      screen.queryByRole("menuitem", { name: "Settings" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("menuitem", { name: /Google sign-in/i }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("menuitem", { name: /Light/i }));
    expect(useWorkbenchStore.getState().theme).toBe("light");
  });

  it("expands a real Git change summary into its changed files", () => {
    render(
      <ChangeSummaryCard
        workspacePath="C:\\project"
        status={{
          branch: "main",
          clean: false,
          entries: [" M src/App.tsx"],
          fileCount: 1,
          additions: 12,
          deletions: 3,
          files: [
            {
              path: "src/App.tsx",
              status: "M",
              additions: 12,
              deletions: 3,
            },
          ],
        }}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /1 files changed/i }));
    expect(screen.getByText("src/App.tsx")).toBeInTheDocument();
    expect(screen.getAllByText("+12")).toHaveLength(2);
    expect(screen.getAllByText("-3")).toHaveLength(2);
  });

  it("navigates to every implemented sidebar section", async () => {
    renderApp();
    const routes: ReadonlyArray<readonly [string, string]> = [
      ["Workspaces", "Workspace"],
      ["Git changes", "Git status"],
      ["Agents", "Agents"],
      ["MCP servers", "MCP servers"],
      ["Browser", "Internal browser"],
    ];
    for (const [buttonName, headingName] of routes) {
      fireEvent.click(screen.getByRole("button", { name: buttonName }));
      expect(
        await screen.findByRole("heading", { name: headingName }),
      ).toBeInTheDocument();
    }
  });

  it("applies quick-start prompts to the real composer", () => {
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /Review a project/i }));
    expect(
      screen.getByRole("textbox", { name: /Give HAWK a task/i }),
    ).toHaveValue(
      "Review the open project, summarize its structure, and identify the most important issues with a verifiable plan.",
    );
  });

  it("uses the verified visual model on legacy Alibaba endpoints", () => {
    expect(
      resolveChatModel(
        "qwen3.7-max",
        "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        true,
      ),
    ).toBe("qwen3.7-plus");
    expect(
      resolveChatModel(
        "qwen3.7-max",
        "https://workspace.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
        true,
      ),
    ).toBe("qwen3.7-max");
  });

  it("saves an edited prompt block back into the assistant message", () => {
    useWorkbenchStore.setState({
      agentState: "idle",
      messages: [
        {
          id: "prompt-message",
          role: "assistant",
          content: "Here is your prompt:\n\n```prompt\nWrite a unit test\n\n```",
          createdAt: new Date().toISOString(),
          durationMs: 500,
        },
      ],
    });
    renderApp();
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByDisplayValue("Write a unit test"), {
      target: { value: "Write two unit tests" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(useWorkbenchStore.getState().messages[0]!.content).toContain(
      "Write two unit tests",
    );
  });

  it("keeps project chats, general chat, and removal behavior separate", () => {
    const actions = useWorkbenchStore.getState();
    actions.setWorkspace("C:\\proj-a", "Project A");
    useWorkbenchStore.getState().addUserMessage("First message in A");
    useWorkbenchStore.getState().createConversation();
    useWorkbenchStore.getState().addUserMessage("Second chat in A");
    useWorkbenchStore.getState().setWorkspace("C:\\proj-b", "Project B");
    expect(useWorkbenchStore.getState().messages).toHaveLength(0);
    useWorkbenchStore.getState().addUserMessage("Message in B");
    useWorkbenchStore.getState().openGeneralChat();
    expect(useWorkbenchStore.getState().messages).toHaveLength(0);
    useWorkbenchStore.getState().addUserMessage("General message");
    useWorkbenchStore.getState().setWorkspace("C:\\proj-a", "Project A");
    expect(useWorkbenchStore.getState().messages.at(-1)?.content).toBe(
      "Second chat in A",
    );
    useWorkbenchStore.getState().removeWorkspace("C:\\proj-a");
    expect(
      useWorkbenchStore.getState().recentProjects.some(
        (project) => project.path === "C:\\proj-a",
      ),
    ).toBe(false);
    useWorkbenchStore.getState().setWorkspace("C:\\proj-a", "Project A");
    const restored = useWorkbenchStore.getState();
    expect(restored.conversations).toHaveLength(2);
    expect(restored.messages.at(-1)?.content).toBe("Second chat in A");
  });

  it("shows live reasoning stages while the agent works", () => {
    useWorkbenchStore.setState({
      agentState: "running",
      messages: [
        {
          id: "stage-message",
          role: "assistant",
          content: "",
          createdAt: new Date().toISOString(),
          activities: [
            {
              id: "read-app",
              tool: "read_file",
              state: "running",
              detail: "Reading the application shell",
              filePath: "src/App.tsx",
            },
          ],
        },
      ],
    });
    renderApp();
    expect(screen.getByText("Inspecting the project")).toBeInTheDocument();
    expect(screen.getByText("Understanding the request")).toBeInTheDocument();
    expect(screen.getByText("src/App.tsx")).toBeInTheDocument();
  });

  it("queues instructions sent while the agent is working", () => {
    useWorkbenchStore.setState({ agentState: "running" });
    renderApp();
    const input = screen.getByRole("textbox", { name: /Give HAWK a task/i });
    fireEvent.change(input, { target: { value: "Add tests too" } });
    fireEvent.keyDown(input, { key: "Enter" });
    const [instruction] = useWorkbenchStore.getState().pendingInstructions;
    expect(instruction?.text).toBe("Add tests too");
    expect(
      useWorkbenchStore.getState().messages.at(-1)?.content,
    ).toBe("Add tests too");
    expect(screen.getByText(/Instructions during the task/i)).toBeInTheDocument();
  });

  it("runs queued instructions as a follow-up turn once the agent is idle", () => {
    const actions = useWorkbenchStore.getState();
    actions.addUserMessage("Original task");
    actions.addPendingInstruction("Also add tests");
    actions.addPendingInstruction("Update the README");
    expect(
      useWorkbenchStore.getState().pendingInstructions,
    ).toHaveLength(2);
    expect(
      useWorkbenchStore.getState().pendingInstructions[1]!.text,
    ).toBe("Update the README");
    renderApp();
    expect(useWorkbenchStore.getState().pendingInstructions).toHaveLength(0);
    expect(useWorkbenchStore.getState().messages.length).toBe(2);
  });

  it("edits, reorders, and removes queued instructions", () => {
    const actions = useWorkbenchStore.getState();
    actions.addPendingInstruction("First");
    actions.addPendingInstruction("Second");
    actions.addPendingInstruction("Third");
    const [first, second, third] =
      useWorkbenchStore.getState().pendingInstructions;
    actions.movePendingInstruction(third!.id, "up");
    expect(
      useWorkbenchStore.getState().pendingInstructions[1]!.id,
    ).toBe(third!.id);
    actions.updatePendingInstruction(second!.id, "Second updated");
    expect(
      useWorkbenchStore.getState().pendingInstructions.find(
        (item) => item.id === second!.id,
      )?.text,
    ).toBe("Second updated");
    actions.removePendingInstruction(first!.id);
    expect(useWorkbenchStore.getState().pendingInstructions).toHaveLength(2);
  });

  it("offers a continue action for an interrupted response", () => {
    useWorkbenchStore.setState({
      agentState: "paused",
      messages: [
        {
          id: "partial-message",
          role: "assistant",
          content: "Partial answer",
          createdAt: new Date().toISOString(),
          error: "Network error",
          continuable: true,
        },
      ],
    });
    renderApp();
    expect(
      screen.getByRole("button", { name: /Continue response/i }),
    ).toBeInTheDocument();
  });

  it("offers to resume an interrupted task after a restart", () => {
    window.localStorage.clear();
    const assistantId = useWorkbenchStore.getState().beginAssistantMessage();
    useWorkbenchStore.getState().appendAssistantDelta(
      assistantId,
      "Partial text",
    );
    useWorkbenchStore
      .getState()
      .failAssistantMessage(assistantId, "Network error", true);
    renderApp();
    const banner = screen.getByLabelText("Unfinished task found");
    expect(
      within(banner).getByRole("button", { name: /Continue task/i }),
    ).toBeInTheDocument();
    expect(
      within(banner).getByRole("button", { name: /Review state/i }),
    ).toBeInTheDocument();
    fireEvent.click(within(banner).getByRole("button", { name: /Dismiss/i }));
    expect(window.localStorage.getItem("hawk.checkpoint.v1")).toBeNull();
  });

  it("shows an offline banner when the connection is unavailable", () => {
    useWorkbenchStore.setState({
      offline: true,
      agentState: "paused",
      messages: [
        {
          id: "user-message",
          role: "user",
          content: "Fix the sidebar",
          createdAt: new Date().toISOString(),
        },
      ],
    });
    renderApp();
    expect(
      screen.getByText("Connection unavailable"),
    ).toBeInTheDocument();
  });
});
