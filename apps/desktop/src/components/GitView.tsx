import { GitBranch, RefreshCw } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { getWorkspaceGitStatus } from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";
import { ChangeSummaryCard } from "./ChangeSummaryCard";

export function GitView() {
  const { t } = useTranslation();
  const workspacePath = useWorkbenchStore((state) => state.workspacePath);
  const setActiveView = useWorkbenchStore((state) => state.setActiveView);
  const statusQuery = useQuery({
    queryKey: ["git-status", workspacePath],
    queryFn: () => getWorkspaceGitStatus(workspacePath ?? ""),
    enabled: Boolean(workspacePath),
    retry: false,
  });
  const status = statusQuery.data;

  return (
    <main className="section-view" aria-labelledby="git-title">
      <header className="section-header">
        <div>
          <span>GIT</span>
          <h1 id="git-title">{t("git.title")}</h1>
          <p>{statusQuery.error ? String(statusQuery.error) : t("git.body")}</p>
        </div>
        {workspacePath ? (
          <button
            className="secondary-inline"
            type="button"
            onClick={() => void statusQuery.refetch()}
          >
            <RefreshCw
              size={15}
              className={statusQuery.isFetching ? "spin" : ""}
            />
            {t("workspace.refresh")}
          </button>
        ) : null}
      </header>
      {!workspacePath ? (
        <div className="empty-section">
          <GitBranch size={28} />
          <p>{t("git.empty")}</p>
          <button type="button" onClick={() => setActiveView("files")}>
            {t("welcome.open")}
          </button>
        </div>
      ) : (
        <section className="data-card">
          <div className="git-summary">
            <span>
              <GitBranch size={17} />
              {status?.branch ?? "—"}
            </span>
            <strong data-clean={status?.clean}>
              {status?.clean
                ? t("git.clean")
                : t("git.changed", { count: status?.entries.length ?? 0 })}
            </strong>
          </div>
          {status && workspacePath && !status.clean ? (
            <ChangeSummaryCard
              status={status}
              workspacePath={workspacePath}
              compact
            />
          ) : (
            <div className="git-entries">
              <p>{t("git.noChanges")}</p>
            </div>
          )}
        </section>
      )}
    </main>
  );
}
