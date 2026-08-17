import { X } from "lucide-react";
import { useEffect } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { AuthView } from "./components/AuthView";
import { Composer } from "./components/Composer";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { WorkbenchView } from "./components/WorkbenchView";
import { useWorkbenchStore } from "./store/workbench";

export function App() {
  const authenticated = useWorkbenchStore((state) => state.authenticated);
  const sidebarOpen = useWorkbenchStore((state) => state.sidebarOpen);
  const activeView = useWorkbenchStore((state) => state.activeView);
  const theme = useWorkbenchStore((state) => state.theme);
  const notice = useWorkbenchStore((state) => state.notice);
  const setNotice = useWorkbenchStore((state) => state.setNotice);
  const setOffline = useWorkbenchStore((state) => state.setOffline);

  useEffect(() => {
    const handleOnline = () => setOffline(false);
    const handleOffline = () => setOffline(true);
    window.addEventListener("online", handleOnline);
    window.addEventListener("offline", handleOffline);
    return () => {
      window.removeEventListener("online", handleOnline);
      window.removeEventListener("offline", handleOffline);
    };
  }, [setOffline]);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timeout);
  }, [notice, setNotice]);

  useEffect(() => {
    const media = window.matchMedia?.("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      document.documentElement.dataset.theme =
        theme === "system" ? (media?.matches ? "dark" : "light") : theme;
    };
    applyTheme();
    media?.addEventListener("change", applyTheme);
    return () => media?.removeEventListener("change", applyTheme);
  }, [theme]);

  if (!authenticated) return <AuthView />;

  return (
    <div className="app-shell" data-sidebar={sidebarOpen}>
      <Sidebar />
      <div className="main-stage">
        <TopBar />
        <div className="main-content">
          <WorkbenchView />
          {activeView === "tasks" ? <Composer /> : null}
        </div>
      </div>
      <CommandPalette />
      {notice ? (
        <div className="toast" role="status">
          <span>{notice}</span>
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => setNotice(null)}
          >
            <X size={15} />
          </button>
        </div>
      ) : null}
    </div>
  );
}
