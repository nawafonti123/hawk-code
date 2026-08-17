import { FileSearch, GitBranch, Network, ShieldCheck, TestTube2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useWorkbenchStore } from "../store/workbench";

const skills = [
  { id: "hawk-graph", icon: Network },
  { id: "project-analysis", icon: FileSearch },
  { id: "git-review", icon: GitBranch },
  { id: "test-planning", icon: TestTube2 },
  { id: "security-review", icon: ShieldCheck },
] as const;

export function SkillsView() {
  const { t } = useTranslation();
  const enabled = useWorkbenchStore((state) => state.enabledSkills);
  const toggle = useWorkbenchStore((state) => state.toggleSkill);
  return (
    <main className="section-view" aria-labelledby="skills-title">
      <header className="section-header">
        <div>
          <span>SKILLS</span>
          <h1 id="skills-title">{t("skills.title")}</h1>
          <p>{t("skills.body")}</p>
        </div>
      </header>
      <div className="selection-grid">
        {skills.map(({ id, icon: Icon }) => {
          const active = enabled.includes(id);
          return (
            <button
              type="button"
              key={id}
              data-active={active}
              onClick={() => toggle(id)}
            >
              <Icon size={20} />
              <span>
                <strong>{t(`skills.${id}`)}</strong>
                <small>{t(`skills.${id}Hint`)}</small>
              </span>
              <em>{active ? t("skills.on") : t("skills.off")}</em>
            </button>
          );
        })}
      </div>
    </main>
  );
}
