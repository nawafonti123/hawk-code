import { FolderOpen, RefreshCw } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { getWorkspaceSummary, pickWorkspace } from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";

export function WorkspaceView() {
  const { t } = useTranslation();
  const workspacePath = useWorkbenchStore((state) => state.workspacePath);
  const workspaceName = useWorkbenchStore((state) => state.workspaceName);
  const setWorkspace = useWorkbenchStore((state) => state.setWorkspace);
  const setNotice = useWorkbenchStore((state) => state.setNotice);
  const summaryQuery = useQuery({
    queryKey: ["workspace-summary", workspacePath],
    queryFn: () => getWorkspaceSummary(workspacePath ?? ""),
    enabled: Boolean(workspacePath),
  });

  const openWorkspace = async () => {
    const result = await pickWorkspace();
    if (result.kind === "selected") {
      setWorkspace(
        result.workspace.canonicalPath,
        result.workspace.displayName,
      );
    } else if (result.kind === "desktop-required") {
      setNotice(t("welcome.desktopOnly"));
    }
  };

  return (
    <main className="section-view" aria-labelledby="workspace-title">
      <header className="section-header">
        <div>
          <span>WORKSPACE</span>
          <h1 id="workspace-title">{t("workspace.title")}</h1>
          <p>{t("workspace.body")}</p>
        </div>
        <button
          className="primary-inline"
          type="button"
          onClick={() => void openWorkspace()}
        >
          <FolderOpen size={16} />
          {t("welcome.open")}
        </button>
      </header>
      {workspacePath ? (
        <section className="data-card">
          <div className="data-card__title">
            <div>
              <strong>{workspaceName}</strong>
              <small>{workspacePath}</small>
            </div>
            <button
              className="icon-button"
              type="button"
              onClick={() => void summaryQuery.refetch()}
              aria-label={t("workspace.refresh")}
            >
              <RefreshCw
                size={16}
                className={summaryQuery.isFetching ? "spin" : ""}
              />
            </button>
          </div>
          <div className="metric-grid">
            <div>
              <strong>
                {summaryQuery.data?.fileCount.toLocaleString() ?? "—"}
              </strong>
              <span>{t("workspace.files")}</span>
            </div>
            <div>
              <strong>
                {summaryQuery.data?.directoryCount.toLocaleString() ?? "—"}
              </strong>
              <span>{t("workspace.directories")}</span>
            </div>
            <div>
              <strong>
                {summaryQuery.data?.frameworks.join(" · ") || "—"}
              </strong>
              <span>{t("workspace.stack")}</span>
            </div>
          </div>
        </section>
      ) : (
        <EmptySection
          text={t("workspace.empty")}
          action={t("welcome.open")}
          onAction={() => void openWorkspace()}
        />
      )}
    </main>
  );
}

function EmptySection({
  text,
  action,
  onAction,
}: {
  text: string;
  action: string;
  onAction: () => void;
}) {
  return (
    <div className="empty-section">
      <FolderOpen size={28} />
      <p>{text}</p>
      <button type="button" onClick={onAction}>
        {action}
      </button>
    </div>
  );
}
