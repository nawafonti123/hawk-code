import { Bot, Braces, ClipboardList, ScanSearch } from "lucide-react";
import { useTranslation } from "react-i18next";
import { type AgentId, useWorkbenchStore } from "../store/workbench";

const agents = [
  { id: "coordinator", icon: Bot },
  { id: "planner", icon: ClipboardList },
  { id: "code", icon: Braces },
  { id: "review", icon: ScanSearch },
] satisfies Array<{ id: AgentId; icon: typeof Bot }>;

export function AgentsView() {
  const { t } = useTranslation();
  const selected = useWorkbenchStore((state) => state.selectedAgent);
  const setSelected = useWorkbenchStore((state) => state.setSelectedAgent);
  return (
    <main className="section-view" aria-labelledby="agents-title">
      <header className="section-header">
        <div>
          <span>AGENTS</span>
          <h1 id="agents-title">{t("agents.title")}</h1>
          <p>{t("agents.body")}</p>
        </div>
      </header>
      <div className="selection-grid">
        {agents.map(({ id, icon: Icon }) => (
          <button
            type="button"
            key={id}
            data-active={selected === id}
            onClick={() => setSelected(id)}
          >
            <Icon size={20} />
            <span>
              <strong>{t(`agents.${id}`)}</strong>
              <small>{t(`agents.${id}Hint`)}</small>
            </span>
            <em>
              {selected === id ? t("agents.selected") : t("agents.select")}
            </em>
          </button>
        ))}
      </div>
    </main>
  );
}
