import {
  Bot,
  FolderOpen,
  GitBranch,
  ListTodo,
  Search,
  Settings,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { pickWorkspace, stopAllTasks } from "../lib/ipc";
import { useWorkbenchStore, type RailView } from "../store/workbench";

const commands = [
  { id: "new", labelKey: "command.newTask", hint: "Ctrl N", icon: ListTodo },
  {
    id: "open",
    labelKey: "command.openWorkspace",
    hint: "Ctrl O",
    icon: FolderOpen,
  },
  {
    id: "git",
    labelKey: "nav.git",
    hint: "/git",
    icon: GitBranch,
    view: "git",
  },
  {
    id: "agents",
    labelKey: "nav.agents",
    hint: "/agents",
    icon: Bot,
    view: "agents",
  },
  {
    id: "settings",
    labelKey: "nav.settings",
    hint: "Ctrl ,",
    icon: Settings,
    view: "settings",
  },
] satisfies Array<{
  id: string;
  labelKey: string;
  hint: string;
  icon: typeof Search;
  view?: RailView;
}>;

export function CommandPalette() {
  const { t } = useTranslation();
  const open = useWorkbenchStore((state) => state.commandPaletteOpen);
  const sidebarOpen = useWorkbenchStore((state) => state.sidebarOpen);
  const setOpen = useWorkbenchStore((state) => state.setCommandPaletteOpen);
  const setSidebarOpen = useWorkbenchStore((state) => state.setSidebarOpen);
  const setActiveView = useWorkbenchStore((state) => state.setActiveView);
  const setWorkspace = useWorkbenchStore((state) => state.setWorkspace);
  const clearTask = useWorkbenchStore((state) => state.clearTask);
  const setAgentState = useWorkbenchStore((state) => state.setAgentState);
  const setNotice = useWorkbenchStore((state) => state.setNotice);
  const [query, setQuery] = useState("");

  const openWorkspace = useCallback(async () => {
    const result = await pickWorkspace();
    if (result.kind === "selected") {
      setWorkspace(
        result.workspace.canonicalPath,
        result.workspace.displayName,
      );
      setActiveView("files");
    } else if (result.kind === "desktop-required") {
      setNotice(t("welcome.desktopOnly"));
    }
  }, [setActiveView, setNotice, setWorkspace, t]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const control = event.ctrlKey || event.metaKey;
      if (
        control &&
        (event.key.toLowerCase() === "k" ||
          (event.shiftKey && event.key.toLowerCase() === "p"))
      ) {
        event.preventDefault();
        setOpen(!open);
      } else if (control && event.key.toLowerCase() === "n") {
        event.preventDefault();
        clearTask();
      } else if (control && event.key.toLowerCase() === "o") {
        event.preventDefault();
        void openWorkspace();
      } else if (control && event.key.toLowerCase() === "b") {
        event.preventDefault();
        setSidebarOpen(!sidebarOpen);
      } else if (control && event.key === ".") {
        event.preventDefault();
        void stopAllTasks().then(() => setAgentState("idle"));
      } else if (event.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    clearTask,
    open,
    openWorkspace,
    setAgentState,
    setOpen,
    setSidebarOpen,
    sidebarOpen,
  ]);

  const filteredCommands = useMemo(
    () =>
      commands.filter((command) =>
        t(command.labelKey).toLowerCase().includes(query.toLowerCase()),
      ),
    [query, t],
  );

  const execute = async (command: (typeof commands)[number]) => {
    if (command.id === "new") clearTask();
    else if (command.id === "open") await openWorkspace();
    else if (command.view) setActiveView(command.view);
    setOpen(false);
    setQuery("");
  };

  if (!open) return null;
  return (
    <div
      className="palette-backdrop"
      role="presentation"
      onMouseDown={() => setOpen(false)}
    >
      <section
        className="command-palette"
        role="dialog"
        aria-modal="true"
        aria-label={t("command.title")}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <label className="command-palette__search">
          <Search size={18} />
          <input
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("command.search")}
          />
          <kbd>ESC</kbd>
        </label>
        <div className="command-palette__group">
          <span>{t("command.commands")}</span>
          {filteredCommands.map((command) => {
            const Icon = command.icon;
            return (
              <button
                key={command.id}
                type="button"
                onClick={() => void execute(command)}
              >
                <Icon size={17} />
                <span>{t(command.labelKey)}</span>
                <kbd>{command.hint}</kbd>
              </button>
            );
          })}
        </div>
      </section>
    </div>
  );
}
