import {
  CheckCircle2,
  KeyRound,
  Languages,
  LoaderCircle,
  Monitor,
  Moon,
  ShieldCheck,
  Sun,
  Trash2,
  Upload,
} from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  HAWK_MODELS,
  type ChatMessage,
  type PermissionProfile,
  type ProviderStatus,
  type ThemeMode,
} from "@hawk-code/shared-types";
import {
  availableLanguages,
  englishSourceTranslations,
  installLanguagePack,
  type HawkLanguagePack,
} from "../i18n";
import {
  deleteQwenApiKey,
  getQwenProviderStatus,
  pickLanguagePackFile,
  saveQwenApiKey,
  streamQwenChat,
  testQwenConnection,
} from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";

const THEMES: ReadonlyArray<{ id: ThemeMode; icon: typeof Monitor }> = [
  { id: "system", icon: Monitor },
  { id: "light", icon: Sun },
  { id: "dark", icon: Moon },
];

const PERMISSIONS: ReadonlyArray<PermissionProfile> = ["ask", "auto", "full"];

export function SettingsView() {
  const { i18n, t } = useTranslation();
  const activeModel = useWorkbenchStore((state) => state.activeModel);
  const baseUrl = useWorkbenchStore((state) => state.hawkBaseUrl);
  const permission = useWorkbenchStore((state) => state.permissionProfile);
  const theme = useWorkbenchStore((state) => state.theme);
  const setActiveModel = useWorkbenchStore((state) => state.setActiveModel);
  const setBaseUrl = useWorkbenchStore((state) => state.setHawkBaseUrl);
  const setPermission = useWorkbenchStore(
    (state) => state.setPermissionProfile,
  );
  const setTheme = useWorkbenchStore((state) => state.setTheme);
  const setNotice = useWorkbenchStore((state) => state.setNotice);
  const [apiKey, setApiKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [connection, setConnection] = useState<string | null>(null);
  const [languages, setLanguages] = useState(availableLanguages);
  const [languageBusy, setLanguageBusy] = useState(false);
  const statusQuery = useQuery<ProviderStatus>({
    queryKey: ["qwen-provider-status"],
    queryFn: getQwenProviderStatus,
    retry: false,
  });
  const status = statusQuery.data ?? {
    configured: false,
    source: "none",
    maskedKey: null,
  };
  const isModalModel = activeModel === "qwen3-coder-30b-a3b-instruct";

  const saveKey = async () => {
    setBusy(true);
    try {
      await saveQwenApiKey(apiKey);
      await statusQuery.refetch();
      setApiKey("");
      setNotice(t("settings.keySaved"));
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const testConnection = async () => {
    setBusy(true);
    setConnection(null);
    try {
      const result = await testQwenConnection({ baseUrl, model: activeModel });
      setConnection(
        `${result.model} · ${result.latencyMs}ms · ${result.usage.totalTokens} tokens`,
      );
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const deleteKey = async () => {
    setBusy(true);
    try {
      await deleteQwenApiKey();
      await statusQuery.refetch();
      setNotice(t("settings.keyDeleted"));
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const changeLanguage = async (locale: string) => {
    const language = languages.find((item) => item.locale === locale);
    if (!language) return;
    if (language.installed) {
      await i18n.changeLanguage(locale);
      return;
    }
    if (!status.configured) {
      setNotice(t("language.requiresQwen"));
      return;
    }

    setLanguageBusy(true);
    try {
      const source = englishSourceTranslations();
      let translated = "";
      const message: ChatMessage = {
        id: crypto.randomUUID(),
        role: "user",
        content: JSON.stringify(source),
        createdAt: new Date().toISOString(),
      };
      await streamQwenChat(
        { baseUrl, model: "qwen3.7-plus" },
        [message],
        [
          "You are the HAWK Code localization compiler.",
          `Translate every JSON value into ${language.name} (${language.locale}).`,
          "Return only one valid flat JSON object with exactly the same keys.",
          "Preserve {{placeholders}}, product names, model IDs, keyboard keys, URLs, and technical acronyms.",
          "Do not use Markdown fences and do not omit any key.",
        ].join(" "),
        (delta) => {
          translated += delta;
        },
      );
      const translations = parseTranslationObject(translated, source);
      const pack: HawkLanguagePack = {
        locale: language.locale,
        name: language.name,
        direction: language.direction,
        translations,
      };
      installLanguagePack(pack);
      setLanguages(availableLanguages());
      await i18n.changeLanguage(language.locale);
      setNotice(t("language.imported", { language: language.name }));
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setLanguageBusy(false);
    }
  };

  const importLanguage = async () => {
    try {
      const raw = await pickLanguagePackFile();
      if (!raw) return;
      const pack = JSON.parse(raw) as HawkLanguagePack;
      installLanguagePack(pack);
      setLanguages(availableLanguages());
      await i18n.changeLanguage(pack.locale);
      setNotice(t("language.imported", { language: pack.name }));
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <main className="settings-view" aria-labelledby="settings-title">
      <div className="settings-view__header">
        <span>HAWK CODE</span>
        <h1 id="settings-title">{t("settings.title")}</h1>
        <p>{t("settings.body")}</p>
      </div>

      <section
        className="settings-section"
        aria-labelledby="appearance-settings"
      >
        <h2 id="appearance-settings">{t("settings.appearance")}</h2>
        <div className="settings-card">
          <div className="setting-row">
            <Sun size={18} />
            <span>
              <strong>{t("settings.theme")}</strong>
              <small>{t("settings.themeHint")}</small>
            </span>
            <div className="segmented-control" aria-label={t("settings.theme")}>
              {THEMES.map(({ id, icon: Icon }) => (
                <button
                  key={id}
                  type="button"
                  data-active={theme === id}
                  onClick={() => setTheme(id)}
                >
                  <Icon size={14} />
                  <span>{t(`theme.${id}`)}</span>
                </button>
              ))}
            </div>
          </div>
          <label className="setting-row" htmlFor="interface-language">
            <Languages size={18} />
            <span>
              <strong>{t("settings.language")}</strong>
              <small>{t("settings.languageHint")}</small>
            </span>
            <div className="language-control">
              <select
                id="interface-language"
                className="setting-value"
                value={i18n.language}
                disabled={languageBusy}
                onChange={(event) => void changeLanguage(event.target.value)}
              >
                {languages.map((language) => (
                  <option key={language.locale} value={language.locale}>
                    {language.name}
                    {language.verified ? ` — ${t("language.verified")}` : ""}
                    {!language.installed ? ` — ${t("language.generate")}` : ""}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="secondary-inline"
                onClick={() => void importLanguage()}
              >
                <Upload size={14} />
                {t("language.import")}
              </button>
            </div>
          </label>
          <p className="settings-explanation">{t("language.more")}</p>
        </div>
      </section>

      <section
        className="settings-section"
        aria-labelledby="permission-settings"
      >
        <h2 id="permission-settings">{t("settings.permission")}</h2>
        <div className="settings-card permission-settings-list">
          {PERMISSIONS.map((item) => (
            <button
              key={item}
              className="permission-setting"
              type="button"
              data-active={permission === item}
              data-danger={item === "full"}
              onClick={() => setPermission(item)}
            >
              <ShieldCheck size={18} />
              <span>
                <strong>{t(`permissions.${item}`)}</strong>
                <small>{t(`permissions.${item}Detail`)}</small>
              </span>
              {permission === item ? <CheckCircle2 size={17} /> : null}
            </button>
          ))}
        </div>
      </section>

      <section className="settings-section" aria-labelledby="qwen-settings">
        <h2 id="qwen-settings">{t("settings.qwen")}</h2>
        <div className="provider-card">
          <div className="provider-card__status">
            <div>
              <span className="provider-logo">{isModalModel ? "M" : "Q"}</span>
              <span>
                <strong>{isModalModel ? "Hawk K3 · Coder" : "Hawk K3 · Cloud"}</strong>
                <small>
                  {status.configured
                    ? `${t("settings.connectedKey")} ${status.maskedKey ?? ""} · ${status.source}`
                    : t("settings.noKey")}
                </small>
              </span>
            </div>
            <em data-connected={status.configured}>
              {status.configured
                ? t("settings.configured")
                : t("settings.required")}
            </em>
          </div>
          <label className="field">
            <span>{t("settings.apiKey")}</span>
            <input
              type="password"
              autoComplete="off"
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder="sk-••••••••"
            />
          </label>
          <label className="field">
            <span>Base URL</span>
            <input
              value={baseUrl}
              onChange={(event) => setBaseUrl(event.target.value)}
              dir="ltr"
            />
          </label>
          <label className="field">
            <span>{t("settings.model")}</span>
            <select
              value={activeModel}
              onChange={(event) => {
                const model = event.target.value as typeof activeModel;
                setActiveModel(model);
                if (model === "qwen3-coder-30b-a3b-instruct") {
                  setBaseUrl("https://mjakcon8-hawk-code--hawk-code-ai-hawkmodel-web.modal.run/v1");
                }
              }}
            >
              {HAWK_MODELS.map((model) => (
                <option key={model.id} value={model.id}>
                  {model.displayName} · {t(`models.${model.mode}`)}
                </option>
              ))}
            </select>
          </label>
          <div className="provider-actions">
            <button
              type="button"
              className="primary-inline"
              disabled={busy || !apiKey.trim()}
              onClick={() => void saveKey()}
            >
              <KeyRound size={15} />
              {t("settings.saveKey")}
            </button>
            <button
              type="button"
              className="secondary-inline"
              disabled={busy || !status.configured}
              onClick={() => void testConnection()}
            >
              {busy ? (
                <LoaderCircle className="spin" size={15} />
              ) : (
                <CheckCircle2 size={15} />
              )}
              {t("settings.testConnection")}
            </button>
            <button
              type="button"
              className="danger-inline"
              disabled={busy || status.source !== "credential-manager"}
              onClick={() => void deleteKey()}
            >
              <Trash2 size={15} />
              {t("settings.deleteKey")}
            </button>
          </div>
          {connection ? (
            <p className="connection-success">
              <CheckCircle2 size={15} />
              {connection}
            </p>
          ) : null}
          <p className="security-note">
            <ShieldCheck size={15} />
            {t("settings.keySecurity")}
          </p>
        </div>
      </section>
    </main>
  );
}

function parseTranslationObject(
  raw: string,
  source: Record<string, string>,
): Record<string, string> {
  const firstBrace = raw.indexOf("{");
  const lastBrace = raw.lastIndexOf("}");
  if (firstBrace < 0 || lastBrace <= firstBrace)
    throw new Error("Hawk K3 did not return a valid language pack.");
  const parsed = JSON.parse(raw.slice(firstBrace, lastBrace + 1)) as unknown;
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
    throw new Error("Qwen returned an invalid language pack object.");
  const translations = parsed as Record<string, unknown>;
  const sourceKeys = Object.keys(source);
  if (
    Object.keys(translations).length !== sourceKeys.length ||
    sourceKeys.some(
      (key) =>
        typeof translations[key] !== "string" ||
        String(translations[key]).length > 4_000,
    )
  ) {
    throw new Error("The generated language pack is incomplete.");
  }
  return Object.fromEntries(
    sourceKeys.map((key) => [key, String(translations[key])]),
  );
}
