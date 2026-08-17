import { lazy, Suspense } from "react";
import { TasksView } from "./TasksView";
import { useWorkbenchStore } from "../store/workbench";

const AgentsView = lazy(() =>
  import("./AgentsView").then((module) => ({ default: module.AgentsView })),
);
const GitView = lazy(() =>
  import("./GitView").then((module) => ({ default: module.GitView })),
);
const SettingsView = lazy(() =>
  import("./SettingsView").then((module) => ({
    default: module.SettingsView,
  })),
);
const McpView = lazy(() =>
  import("./McpView").then((module) => ({ default: module.McpView })),
);
const WorkspaceView = lazy(() =>
  import("./WorkspaceView").then((module) => ({
    default: module.WorkspaceView,
  })),
);
const BrowserView = lazy(() =>
  import("./BrowserView").then((module) => ({ default: module.BrowserView })),
);

export function WorkbenchView() {
  const activeView = useWorkbenchStore((state) => state.activeView);
  const view = (() => {
    switch (activeView) {
      case "files":
        return <WorkspaceView />;
      case "git":
        return <GitView />;
      case "agents":
        return <AgentsView />;
      case "mcp":
        return <McpView />;
      case "browser":
        return <BrowserView />;
      case "settings":
        return <SettingsView />;
      default:
        return <TasksView />;
    }
  })();
  return (
    <Suspense fallback={<div className="view-loading" aria-hidden="true" />}>
      {view}
    </Suspense>
  );
}
