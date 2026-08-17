import { ExternalLink, Globe2, LoaderCircle, RefreshCw, X } from "lucide-react";
import {
  type FormEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { Webview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { resolveBrowserInput } from "../lib/browser-input";
import { isTauriRuntime } from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";

export function BrowserView() {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const webviewRef = useRef<Webview | null>(null);
  const creationTimerRef = useRef<number | null>(null);
  const requestedAddress = useWorkbenchStore((state) => state.browserAddress);
  const [address, setAddress] = useState(
    requestedAddress || "https://example.com",
  );
  const [currentUrl, setCurrentUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const setNotice = useWorkbenchStore((state) => state.setNotice);

  const closeWebview = useCallback(async () => {
    if (creationTimerRef.current !== null) {
      window.clearTimeout(creationTimerRef.current);
      creationTimerRef.current = null;
    }
    const webview = webviewRef.current;
    webviewRef.current = null;
    setCurrentUrl(null);
    setLoading(false);
    if (!webview) return;
    try {
      await webview.close();
    } catch {
      // It may already have closed with the parent view.
    }
  }, []);

  const layoutWebview = useCallback(async () => {
    const container = containerRef.current;
    const webview = webviewRef.current;
    if (!container || !webview) return;
    const rect = container.getBoundingClientRect();
    if (rect.width < 1 || rect.height < 1) return;
    await webview.setPosition(new LogicalPosition(rect.left, rect.top));
    await webview.setSize(new LogicalSize(rect.width, rect.height));
  }, []);

  const openUrl = useCallback(
    async (rawUrl: string) => {
      if (!isTauriRuntime()) {
        setNotice(t("browser.desktopOnly"));
        return;
      }
      const resolvedUrl = resolveBrowserInput(rawUrl);
      if (!resolvedUrl) {
        setNotice(t("browser.invalidUrl"));
        return;
      }
      const url = new URL(resolvedUrl);
      const container = containerRef.current;
      if (!container) return;
      setError(null);
      await closeWebview();
      setLoading(true);
      try {
        const rect = container.getBoundingClientRect();
        const webview = new Webview(
          getCurrentWindow(),
          `hawk-browser-${Date.now()}`,
          {
            url: url.toString(),
            x: rect.left,
            y: rect.top,
            width: rect.width,
            height: rect.height,
            focus: true,
            incognito: false,
          },
        );
        webviewRef.current = webview;
        let created = false;
        await Promise.all([
          webview.once("tauri://created", () => {
            if (webviewRef.current !== webview) return;
            created = true;
            if (creationTimerRef.current !== null) {
              window.clearTimeout(creationTimerRef.current);
              creationTimerRef.current = null;
            }
            setCurrentUrl(url.toString());
            setLoading(false);
            void layoutWebview();
            void webview.setFocus();
          }),
          webview.once("tauri://error", (event) => {
            if (webviewRef.current !== webview) return;
            if (creationTimerRef.current !== null) {
              window.clearTimeout(creationTimerRef.current);
              creationTimerRef.current = null;
            }
            webviewRef.current = null;
            setLoading(false);
            const detail = formatWebviewError(event.payload);
            const message = `${t("browser.openFailed")} ${detail}`.trim();
            setError(message);
            setNotice(message);
          }),
        ]);
        creationTimerRef.current = window.setTimeout(() => {
          if (webviewRef.current !== webview || created) return;
          webviewRef.current = null;
          setLoading(false);
          const message = t("browser.openTimeout");
          setError(message);
          setNotice(message);
          void webview.close();
        }, 12_000);
      } catch (caught) {
        setLoading(false);
        const detail = formatWebviewError(caught);
        const message = `${t("browser.openFailed")} ${detail}`.trim();
        setError(message);
        setNotice(message);
      }
    },
    [closeWebview, layoutWebview, setNotice, t],
  );

  useEffect(() => {
    const handleLayout = () => void layoutWebview();
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => void layoutWebview());
    if (containerRef.current) observer?.observe(containerRef.current);
    window.addEventListener("resize", handleLayout);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", handleLayout);
      void closeWebview();
    };
  }, [closeWebview, layoutWebview]);

  /* Browser requests arrive from slash commands through the shared workbench
     store, so this effect intentionally synchronizes that external command
     with both the address field and the native child webview. */
  /* eslint-disable react-hooks/set-state-in-effect */
  useEffect(() => {
    if (!requestedAddress) return;
    setAddress(requestedAddress);
    void openUrl(requestedAddress);
  }, [openUrl, requestedAddress]);
  /* eslint-enable react-hooks/set-state-in-effect */

  const submit = (event: FormEvent) => {
    event.preventDefault();
    void openUrl(address);
  };

  return (
    <main className="internal-browser" aria-labelledby="browser-title">
      <form className="browser-toolbar" onSubmit={submit}>
        <Globe2 size={16} />
        <h1 id="browser-title" className="sr-only">
          {t("browser.title")}
        </h1>
        <input
          aria-label={t("browser.address")}
          value={address}
          onChange={(event) => setAddress(event.target.value)}
          dir="ltr"
          placeholder={t("browser.addressPlaceholder")}
        />
        <button type="submit" disabled={loading || !address.trim()}>
          {loading ? (
            <LoaderCircle className="spin" size={15} />
          ) : (
            <ExternalLink size={15} />
          )}
          {t("browser.go")}
        </button>
        <button
          type="button"
          disabled={!currentUrl || loading}
          aria-label={t("browser.reload")}
          onClick={() => currentUrl && void openUrl(currentUrl)}
        >
          <RefreshCw size={15} />
        </button>
        <button
          type="button"
          disabled={!currentUrl}
          aria-label={t("browser.close")}
          onClick={() => void closeWebview()}
        >
          <X size={15} />
        </button>
      </form>
      <div className="browser-viewport" ref={containerRef}>
        {!currentUrl && !loading ? (
          <div className="browser-empty">
            <Globe2 size={28} />
            <strong>{t("browser.emptyTitle")}</strong>
            <p>{t("browser.emptyBody")}</p>
            {error ? <p className="browser-error">{error}</p> : null}
            <button type="button" onClick={() => void openUrl(address)}>
              <ExternalLink size={15} />
              {t("browser.openExample")}
            </button>
          </div>
        ) : null}
        {loading ? (
          <div className="browser-loading" role="status" aria-live="polite">
            <LoaderCircle className="spin" size={22} />
            <span>{t("browser.opening")}</span>
          </div>
        ) : null}
      </div>
    </main>
  );
}

function formatWebviewError(value: unknown): string {
  if (value instanceof Error) return value.message;
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}
