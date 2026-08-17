import {
  Bot,
  ChevronUp,
  Folder,
  GitBranch,
  Globe2,
  Languages,
  ListTodo,
  LogOut,
  MessageSquare,
  Moon,
  MoreHorizontal,
  Pencil,
  PlugZap,
  Plus,
  Search,
  Settings,
  Sun,
  Trash2,
  UserRound,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { pickWorkspace } from "../lib/ipc";
import { type RailView, useWorkbenchStore } from "../store/workbench";
import { HawkBrand } from "./HawkBrand";
import { PopoverMenu } from "./PopoverMenu";

const primaryNavigation = [
  { view: "tasks", key: "tasks", icon: ListTodo },
  { view: "files", key: "workspaces", icon: Folder },
  { view: "git", key: "git", icon: GitBranch },
  { view: "agents", key: "agents", icon: Bot },
  { view: "mcp", key: "mcp", icon: PlugZap },
  { view: "browser", key: "browser", icon: Globe2 },
] satisfies Array<{ view: RailView; key: string; icon: typeof ListTodo }>;

function AccountAvatar({
  avatarUrl,
  name,
  large = false,
}: {
  avatarUrl: string | null;
  name: string;
  large?: boolean;
}) {
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const initials = name
    .trim()
    .split(/\s+/u)
    .slice(0, 2)
    .map((part) => part[0])
    .join("")
    .toUpperCase();

  return (
    <span className={`account-avatar${large ? " account-avatar--large" : ""}`}>
      {avatarUrl && failedUrl !== avatarUrl ? (
        <img src={avatarUrl} alt="" onError={() => setFailedUrl(avatarUrl)} />
      ) : initials ? (
        <span className="account-avatar__initials" aria-hidden="true">
          {initials}
        </span>
      ) : (
        <UserRound size={large ? 18 : 16} />
      )}
    </span>
  );
}

export function Sidebar() {
  const { i18n, t } = useTranslation();
  const workspaceName = useWorkbenchStore((state) => state.workspaceName);
  const workspacePath = useWorkbenchStore((state) => state.workspacePath);
  const activeView = useWorkbenchStore((state) => state.activeView);
  const profile = useWorkbenchStore((state) => state.userProfile);
  const theme = useWorkbenchStore((state) => state.theme);
  const setWorkspace = useWorkbenchStore((state) => state.setWorkspace);
  const openGeneralChat = useWorkbenchStore((state) => state.openGeneralChat);
  const createConversation = useWorkbenchStore((state) => state.createConversation);
  const selectConversation = useWorkbenchStore((state) => state.selectConversation);
  const renameConversation = useWorkbenchStore((state) => state.renameConversation);
  const deleteConversation = useWorkbenchStore((state) => state.deleteConversation);
  const removeWorkspace = useWorkbenchStore((state) => state.removeWorkspace);
  const setActiveView = useWorkbenchStore((state) => state.setActiveView);
  const setTheme = useWorkbenchStore((state) => state.setTheme);
  const setNotice = useWorkbenchStore((state) => state.setNotice);
  const setCommandPaletteOpen = useWorkbenchStore(
    (state) => state.setCommandPaletteOpen,
  );
  const conversationId = useWorkbenchStore((state) => state.conversationId);
  const conversations = useWorkbenchStore((state) => state.conversations);
  const recentProjects = useWorkbenchStore((state) => state.recentProjects);
  const logout = useWorkbenchStore((state) => state.logout);

  const openWorkspace = async () => {
    try {
      const result = await pickWorkspace();
      if (result.kind === "selected")
        setWorkspace(
          result.workspace.canonicalPath,
          result.workspace.displayName,
        );
      else if (result.kind === "desktop-required")
        setNotice(t("welcome.desktopOnly"));
    } catch (error) {
      setNotice(
        error instanceof Error ? error.message : t("welcome.openError"),
      );
    }
  };

  return (
    <aside className="sidebar">
      <div className="sidebar__top">
        <div className="sidebar__header">
          <HawkBrand />
        </div>

        <button
          className="new-task-button"
          type="button"
          onClick={createConversation}
        >
          <Plus size={16} />
          <span>{t("sidebar.newTask")}</span>
        </button>

        <button
          className="sidebar-search"
          type="button"
          onClick={() => setCommandPaletteOpen(true)}
        >
          <Search size={15} aria-hidden="true" />
          <span>{t("sidebar.search")}</span>
          <kbd>Ctrl K</kbd>
        </button>
      </div>

      <div className="sidebar__scroll">
        <div className="sidebar__eyebrow">
          <span>{t("sidebar.navigation")}</span>
        </div>

        <nav className="sidebar-nav" aria-label={t("sidebar.navigation")}>
          {primaryNavigation.map(({ view, key, icon: Icon }) => {
            const active = activeView === view;
            return (
              <button
                key={view}
                type="button"
                data-active={active}
                aria-current={active ? "page" : undefined}
                onClick={() => setActiveView(view)}
              >
                <span className="sidebar-nav__icon" aria-hidden="true">
                  <Icon size={16} />
                </span>
                <span>{t(`nav.${key}`)}</span>
              </button>
            );
          })}
        </nav>

        <section className="sidebar-lists">
          <div className="sidebar__section-heading">
            <span>{t("sidebar.projects")}</span>
            <span className="sidebar__eyebrow-count" aria-hidden="true">
              {recentProjects.length}
            </span>
            <button
              type="button"
              title={t("sidebar.addProject")}
              aria-label={t("sidebar.addProject")}
              onClick={() => void openWorkspace()}
            >
              <Plus size={14} />
            </button>
          </div>

          <button
            type="button"
            className="conversation-row conversation-row--general"
            data-active={!workspaceName}
            onClick={openGeneralChat}
          >
            <span className="conversation-row__line">
              <span className="conversation-row__icon" aria-hidden="true">
                <MessageSquare size={14} />
              </span>
              <span>{t("sidebar.generalChat")}</span>
            </span>
            <small>{t("sidebar.generalChatHint")}</small>
          </button>

          {recentProjects.map((project) => (
            <div className="saved-project" key={project.path}>
              <button
                type="button"
                data-active={workspacePath === project.path}
                onClick={() => setWorkspace(project.path, project.name)}
              >
                <span className="project-row__icon" aria-hidden="true">
                  <Folder size={14} />
                </span>
                <span>{project.name}</span>
              </button>
              <button
                type="button"
                className="saved-project__remove"
                title={t("sidebar.removeProject")}
                aria-label={t("sidebar.removeProject")}
                onClick={() => removeWorkspace(project.path)}
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))}

          <div className="sidebar__section-heading sidebar__section-heading--chats">
            <span>
              {workspaceName
                ? t("sidebar.projectChats")
                : t("sidebar.generalChats")}
            </span>
            <span className="sidebar__eyebrow-count" aria-hidden="true">
              {conversations.length}
            </span>
            <button
              type="button"
              title={t("sidebar.newChat")}
              aria-label={t("sidebar.newChat")}
              onClick={createConversation}
            >
              <Plus size={14} />
            </button>
          </div>

          <div className="conversation-list">
            {conversations.map((conversation) => {
              const active = conversation.id === conversationId;
              return (
                <div
                  className="conversation-list__item"
                  data-active={active}
                  key={conversation.id}
                >
                  <button
                    type="button"
                    className="conversation-list__card"
                    data-active={active}
                    aria-current={active ? "true" : undefined}
                    onClick={() => selectConversation(conversation.id)}
                  >
                    <span className="conversation-list__icon" aria-hidden="true">
                      <MessageSquare size={13} />
                    </span>
                    <span className="conversation-list__text">
                      <strong>{conversation.title}</strong>
                      <small>
                        {conversation.messages.length
                          ? t("sidebar.messageCount", {
                              count: conversation.messages.length,
                            })
                          : t("sidebar.emptyChat")}
                      </small>
                    </span>
                  </button>

                  <div className="conversation-list__actions">
                    <PopoverMenu
                      label={t("sidebar.chatOptions")}
                      placement="bottom-start"
                      className="chat-menu"
                      trigger={<MoreHorizontal size={15} />}
                    >
                      {(close) => (
                        <div className="menu-list chat-menu-list">
                          <button
                            type="button"
                            role="menuitem"
                            onClick={() => {
                              const title = window.prompt(
                                t("sidebar.renameChat"),
                                conversation.title,
                              );
                              if (title)
                                renameConversation(conversation.id, title);
                              close();
                            }}
                          >
                            <Pencil size={16} />
                            <span>{t("sidebar.renameChat")}</span>
                          </button>
                          <button
                            type="button"
                            role="menuitem"
                            data-danger
                            onClick={() => {
                              close();
                              deleteConversation(conversation.id);
                            }}
                          >
                            <Trash2 size={16} />
                            <span>{t("sidebar.deleteChat")}</span>
                          </button>
                        </div>
                      )}
                    </PopoverMenu>
                    <button
                      type="button"
                      className="conversation-list__remove"
                      aria-label={t("sidebar.deleteChat")}
                      onClick={() => deleteConversation(conversation.id)}
                    >
                      <Trash2 size={15} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </section>
      </div>

      <div className="sidebar__bottom">
        <button
          className="sidebar-settings"
          type="button"
          data-active={activeView === "settings"}
          aria-current={activeView === "settings" ? "page" : undefined}
          onClick={() => setActiveView("settings")}
        >
          <span className="sidebar-settings__icon" aria-hidden="true">
            <Settings size={16} />
          </span>
          <span>{t("nav.settings")}</span>
        </button>

        <PopoverMenu
          label={t("account.menu")}
          placement="top-start"
          className="account-popover"
          trigger={
            <>
              <AccountAvatar
                avatarUrl={profile.avatarUrl}
                name={profile.name}
              />
              <span className="account-summary">
                <strong>{profile.name}</strong>
                <small>{profile.email ?? t("account.local")}</small>
              </span>
              <ChevronUp size={14} />
            </>
          }
        >
          {(close) => (
            <div className="account-menu">
              <div className="account-menu__profile">
                <AccountAvatar
                  avatarUrl={profile.avatarUrl}
                  name={profile.name}
                  large
                />
                <span>
                  <strong>{profile.name}</strong>
                  <small>{profile.email ?? t("account.localSession")}</small>
                </span>
              </div>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setTheme(theme === "light" ? "dark" : "light");
                  close();
                }}
              >
                {theme === "light" ? <Moon size={16} /> : <Sun size={16} />}
                <span>
                  {theme === "light" ? t("theme.dark") : t("theme.light")}
                </span>
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  void i18n.changeLanguage(
                    i18n.language === "ar" ? "en" : "ar",
                  );
                  close();
                }}
              >
                <Languages size={16} />
                <span>{t("settings.language")}</span>
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  logout();
                }}
              >
                <LogOut size={16} />
                <span>{t("account.logout")}</span>
              </button>
            </div>
          )}
        </PopoverMenu>
      </div>
    </aside>
  );
}
