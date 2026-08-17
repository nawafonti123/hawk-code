import { Check, Command, Languages, PanelLeft, Shield } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useWorkbenchStore } from "../store/workbench";
import { PopoverMenu } from "./PopoverMenu";

export function TopBar() {
  const { i18n, t } = useTranslation();
  const workspaceName = useWorkbenchStore((state) => state.workspaceName);
  const sidebarOpen = useWorkbenchStore((state) => state.sidebarOpen);
  const agentState = useWorkbenchStore((state) => state.agentState);
  const permission = useWorkbenchStore((state) => state.permissionProfile);
  const usage = useWorkbenchStore((state) => state.usage);
  const setSidebarOpen = useWorkbenchStore((state) => state.setSidebarOpen);
  const setCommandPaletteOpen = useWorkbenchStore(
    (state) => state.setCommandPaletteOpen,
  );

  return (
    <header className="topbar">
      <div className="topbar__title-group">
        <button
          className="icon-button"
          type="button"
          onClick={() => setSidebarOpen(!sidebarOpen)}
          aria-label={t("top.toggleSidebar")}
        >
          <PanelLeft size={18} />
        </button>
        <div className="topbar__title">
          <strong>{workspaceName ?? t("top.home")}</strong>
          <span>
            {agentState === "running" ? t("top.running") : t("top.ready")}
          </span>
        </div>
      </div>

      <div className="topbar__actions">
        <span className="usage-state">
          {usage.totalTokens.toLocaleString()} {t("top.tokens")}
        </span>
        <span className="safe-state" data-permission={permission}>
          <Shield size={14} /> {t(`permissions.${permission}`)}
        </span>
        <button
          className="command-button"
          type="button"
          onClick={() => setCommandPaletteOpen(true)}
          aria-label={t("command.title")}
        >
          <Command size={15} />
          <kbd>Ctrl K</kbd>
        </button>
        <PopoverMenu
          label={t("settings.language")}
          placement="bottom-end"
          trigger={<Languages size={18} />}
        >
          {(close) => (
            <div className="menu-list menu-list--compact language-menu">
              <button
                type="button"
                role="menuitemradio"
                aria-checked={i18n.language.startsWith("ar")}
                onClick={() => {
                  void i18n.changeLanguage("ar");
                  close();
                }}
              >
                <span>
                  <strong>العربية</strong>
                </span>
                {i18n.language.startsWith("ar") ? <Check size={15} /> : null}
              </button>
              <button
                type="button"
                role="menuitemradio"
                aria-checked={i18n.language.startsWith("en")}
                onClick={() => {
                  void i18n.changeLanguage("en");
                  close();
                }}
              >
                <span>
                  <strong>English</strong>
                </span>
                {i18n.language.startsWith("en") ? <Check size={15} /> : null}
              </button>
            </div>
          )}
        </PopoverMenu>
      </div>
    </header>
  );
}
