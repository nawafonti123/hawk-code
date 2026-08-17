import {
  CheckCircle2,
  FolderOpen,
  LoaderCircle,
  PlugZap,
  ShieldAlert,
  Wrench,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { McpProbeResult } from "@hawk-code/shared-types";
import {
  callBuiltInMcpTool,
  isTauriRuntime,
  pickMcpExecutable,
  probeBuiltInMcp,
  probeMcpServer,
} from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";

export function McpView() {
  const { t } = useTranslation();
  const workspacePath = useWorkbenchStore((state) => state.workspacePath);
  const setNotice = useWorkbenchStore((state) => state.setNotice);
  const [name, setName] = useState("Local MCP server");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<McpProbeResult | null>(null);
  const [builtin, setBuiltin] = useState<McpProbeResult | null>(null);
  const [builtinBusy, setBuiltinBusy] = useState(isTauriRuntime());
  const [toolOutput, setToolOutput] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let active = true;
    void probeBuiltInMcp(workspacePath)
      .then((discovered) => {
        if (active) setBuiltin(discovered);
      })
      .catch((error: unknown) => {
        if (active)
          setNotice(error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (active) setBuiltinBusy(false);
      });
    return () => {
      active = false;
    };
  }, [setNotice, workspacePath]);

  const runBuiltIn = async (tool: string) => {
    if (!workspacePath) {
      setNotice(t("mcp.openWorkspace"));
      return;
    }
    setBuiltinBusy(true);
    try {
      const output = await callBuiltInMcpTool(tool, workspacePath);
      setToolOutput(JSON.stringify(output, null, 2));
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setBuiltinBusy(false);
    }
  };

  const chooseExecutable = async () => {
    try {
      const selected = await pickMcpExecutable();
      if (selected) setCommand(selected);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    }
  };

  const connect = async () => {
    setBusy(true);
    setResult(null);
    try {
      const discovered = await probeMcpServer({
        name,
        command,
        args: args
          .split(/\r?\n/)
          .map((value) => value.trim())
          .filter(Boolean),
        workspacePath,
      });
      setResult(discovered);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="section-view" aria-labelledby="mcp-title">
      <header className="section-header">
        <div>
          <span>MCP 2025-11-25</span>
          <h1 id="mcp-title">{t("mcp.title")}</h1>
          <p>{t("mcp.body")}</p>
        </div>
      </header>
      <section className="data-card mcp-built-in" aria-live="polite">
        <div className="data-card__title">
          <div>
            <strong>
              {builtinBusy ? (
                <LoaderCircle className="spin" size={15} />
              ) : (
                <CheckCircle2 size={15} />
              )}
              {t("mcp.builtinTitle")}
            </strong>
            <small>
              {builtin
                ? `${builtin.protocolVersion} · ${builtin.tools.length} tools`
                : t("mcp.builtinConnecting")}
            </small>
          </div>
          <span className="status-pill" data-connected={Boolean(builtin)}>
            {builtin ? t("mcp.active") : t("mcp.connecting")}
          </span>
        </div>
        <div className="mcp-tool-list mcp-tool-list--runnable">
          {builtin?.tools.map((tool) => (
            <article key={tool.name}>
              <Wrench size={16} />
              <span>
                <strong>{tool.name}</strong>
                <small>{tool.description}</small>
              </span>
              <button
                type="button"
                className="secondary-inline"
                disabled={builtinBusy || !workspacePath}
                onClick={() => void runBuiltIn(tool.name)}
              >
                {t("mcp.run")}
              </button>
            </article>
          ))}
        </div>
        {toolOutput ? (
          <pre className="mcp-tool-output">{toolOutput}</pre>
        ) : null}
      </section>

      <details className="mcp-advanced">
        <summary>{t("mcp.advanced")}</summary>
        <section className="data-card mcp-card">
          <div className="mcp-security">
            <ShieldAlert size={17} />
            <span>
              <strong>{t("mcp.consent")}</strong>
              <small>{t("mcp.consentDetail")}</small>
            </span>
          </div>
          <label className="field">
            <span>{t("mcp.name")}</span>
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <div className="field">
            <span>{t("mcp.executable")}</span>
            <div className="file-input-row">
              <input
                value={command}
                readOnly
                placeholder="C:\\path\\to\\server.exe"
              />
              <button
                className="secondary-inline"
                type="button"
                onClick={() => void chooseExecutable()}
              >
                <FolderOpen size={15} />
                {t("mcp.choose")}
              </button>
            </div>
          </div>
          <label className="field field--textarea">
            <span>{t("mcp.args")}</span>
            <textarea
              value={args}
              onChange={(event) => setArgs(event.target.value)}
              rows={3}
              placeholder={t("mcp.argsHint")}
            />
          </label>
          <div className="provider-actions">
            <button
              className="primary-inline"
              type="button"
              disabled={busy || !command || !name.trim()}
              onClick={() => void connect()}
            >
              {busy ? (
                <LoaderCircle className="spin" size={15} />
              ) : (
                <PlugZap size={15} />
              )}
              {t("mcp.connect")}
            </button>
          </div>
        </section>
      </details>
      {result ? (
        <section className="data-card mcp-results" aria-live="polite">
          <div className="data-card__title">
            <div>
              <strong>
                <CheckCircle2 size={15} />
                {result.serverName}
              </strong>
              <small>
                {result.protocolVersion} · {result.tools.length} tools
              </small>
            </div>
          </div>
          <div className="mcp-tool-list">
            {result.tools.length ? (
              result.tools.map((tool) => (
                <article key={tool.name}>
                  <Wrench size={16} />
                  <span>
                    <strong>{tool.name}</strong>
                    <small>{tool.description || t("mcp.noDescription")}</small>
                  </span>
                </article>
              ))
            ) : (
              <p>{t("mcp.noTools")}</p>
            )}
          </div>
        </section>
      ) : null}
    </main>
  );
}
