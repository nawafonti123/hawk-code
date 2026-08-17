import {
  ArrowUpLeft,
  FileSearch,
  FolderOpen,
  Settings2,
  WandSparkles,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { pickWorkspace } from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";

export function WelcomeWorkbench() {
  const { t } = useTranslation();
  const setWorkspace = useWorkbenchStore((state) => state.setWorkspace);
  const setDraft = useWorkbenchStore((state) => state.setComposerDraft);
  const setActiveView = useWorkbenchStore((state) => state.setActiveView);
  const setNotice = useWorkbenchStore((state) => state.setNotice);

  const openWorkspace = async () => {
    try {
      const result = await pickWorkspace();
      if (result.kind === "selected") {
        setWorkspace(
          result.workspace.canonicalPath,
          result.workspace.displayName,
        );
      } else if (result.kind === "desktop-required") {
        setNotice(t("welcome.desktopOnly"));
      }
    } catch (error) {
      setNotice(
        error instanceof Error ? error.message : t("welcome.openError"),
      );
    }
  };

  return (
    <main className="workbench" aria-labelledby="welcome-title">
      <div className="welcome">
        <div className="welcome__intro">
          <span className="welcome__mark">
            <img src="/brand/hawk-code-mark.png" alt="" />
          </span>
          <h1 id="welcome-title">{t("welcome.title")}</h1>
          <p>{t("welcome.body")}</p>
        </div>

        <button
          className="primary-action"
          type="button"
          onClick={() => void openWorkspace()}
        >
          <FolderOpen size={17} />
          <span>{t("welcome.open")}</span>
          <ArrowUpLeft className="primary-action__arrow" size={15} />
        </button>

        <section className="quick-start" aria-label={t("welcome.quickStart")}>
          <button
            type="button"
            onClick={() => setDraft(t("welcome.reviewPrompt"))}
          >
            <FileSearch size={17} />
            <span>
              <strong>{t("welcome.review")}</strong>
              <small>{t("welcome.reviewHint")}</small>
            </span>
          </button>
          <button
            type="button"
            onClick={() => setDraft(t("welcome.fixPrompt"))}
          >
            <WandSparkles size={17} />
            <span>
              <strong>{t("welcome.fix")}</strong>
              <small>{t("welcome.fixHint")}</small>
            </span>
          </button>
          <button type="button" onClick={() => setActiveView("settings")}>
            <Settings2 size={17} />
            <span>
              <strong>{t("welcome.provider")}</strong>
              <small>{t("welcome.providerHint")}</small>
            </span>
          </button>
        </section>
      </div>
    </main>
  );
}
