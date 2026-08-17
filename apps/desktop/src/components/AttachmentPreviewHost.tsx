import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { FileText, ImageIcon, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  IPC_PROTOCOL_VERSION,
  type ChatAttachment,
} from "@hawk-code/shared-types";
import { isTauriRuntime } from "../lib/ipc";
import { useWorkbenchStore } from "../store/workbench";

export function AttachmentPreviewHost() {
  const { t } = useTranslation();
  const messages = useWorkbenchStore((state) => state.messages);
  const composerAttachments = useWorkbenchStore((state) => state.attachments);
  const [generatedAttachments, setGeneratedAttachments] = useState<ChatAttachment[]>([]);
  const [preview, setPreview] = useState<ChatAttachment | null>(null);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ChatAttachment>("attachment://generated", (event) => {
      if (disposed) return;
      const attachment = event.payload;
      if (!isPreviewableImage(attachment)) return;
      setGeneratedAttachments((current) => rememberAttachment(current, attachment));
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const allAttachments = useMemo(
    () => [
      ...generatedAttachments,
      ...composerAttachments,
      ...messages.flatMap((message) => message.attachments ?? []),
    ],
    [composerAttachments, generatedAttachments, messages],
  );

  useEffect(() => {
    const openGeneratedPath = async (path: string) => {
      const existing = [...allAttachments]
        .reverse()
        .find((item) => item.path === path && isPreviewableImage(item));
      if (existing) {
        setPreview(existing);
        return;
      }
      if (!isTauriRuntime()) return;
      try {
        const response = await invoke<ChatAttachment[]>("prepare_attachments", {
          request: {
            protocolVersion: IPC_PROTOCOL_VERSION,
            requestId: crypto.randomUUID(),
            payload: { paths: [path] },
          },
        });
        const attachment = response[0];
        if (!attachment || !isPreviewableImage(attachment)) return;
        setGeneratedAttachments((current) => rememberAttachment(current, attachment));
        setPreview(attachment);
      } catch {
        // The screenshot may have been removed by Playwright cleanup. Keep the
        // click safe instead of navigating the WebView away from HAWK Code.
      }
    };

    const handleClick = (event: MouseEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;

      const pathButton = target.closest<HTMLElement>("[data-hawk-generated-path]");
      if (pathButton) {
        const path = pathButton.dataset.hawkGeneratedPath?.trim();
        if (!path) return;
        event.preventDefault();
        event.stopPropagation();
        void openGeneratedPath(path);
        return;
      }

      const generatedButton = target.closest<HTMLElement>(
        "[data-hawk-generated-attachment]",
      );
      if (generatedButton) {
        const name = generatedButton.dataset.hawkGeneratedAttachment?.trim();
        if (!name) return;
        const attachment = [...generatedAttachments]
          .reverse()
          .find((item) => item.name === name);
        event.preventDefault();
        event.stopPropagation();
        if (attachment) setPreview(attachment);
        return;
      }

      if (target.closest("button")) return;

      const image = target.closest<HTMLImageElement>(
        ".message-attachments img, .attachment-chip img",
      );
      const file = target.closest<HTMLElement>(
        ".message-attachments > span, .attachment-chip strong",
      );
      const name = image?.alt?.trim() || file?.textContent?.trim();
      if (!name) return;

      const attachment = [...allAttachments]
        .reverse()
        .find((item) => item.name === name);
      if (!attachment) return;
      event.preventDefault();
      setPreview(attachment);
    };

    document.addEventListener("click", handleClick, true);
    return () => document.removeEventListener("click", handleClick, true);
  }, [allAttachments, generatedAttachments]);

  useEffect(() => {
    if (!preview) return;
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPreview(null);
    };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [preview]);

  if (!preview) return null;

  return (
    <div
      className="attachment-preview-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) setPreview(null);
      }}
    >
      <section
        className="attachment-preview"
        role="dialog"
        aria-modal="true"
        aria-label={preview.name}
      >
        <header className="attachment-preview__header">
          {preview.kind === "image" ? (
            <ImageIcon size={18} aria-hidden="true" />
          ) : (
            <FileText size={18} aria-hidden="true" />
          )}
          <span className="attachment-preview__meta">
            <strong>{preview.name}</strong>
            <small>
              {preview.mimeType} · {formatBytes(preview.size)}
            </small>
          </span>
          <button
            type="button"
            className="attachment-preview__close"
            onClick={() => setPreview(null)}
            aria-label={t("attachment.preview.close")}
          >
            <X size={18} />
          </button>
        </header>
        <div className="attachment-preview__content">
          {preview.kind === "image" && preview.dataUrl ? (
            <img src={preview.dataUrl} alt={preview.name} />
          ) : preview.kind === "pdf" && preview.dataUrl ? (
            <iframe
              className="attachment-preview__pdf"
              src={preview.dataUrl}
              title={preview.name}
            />
          ) : preview.kind === "text" ? (
            <pre dir="auto">
              {preview.textContent ?? "This file has no previewable text."}
            </pre>
          ) : (
            <div className="attachment-preview__empty">
              This attachment type does not have an internal renderer yet.
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function isPreviewableImage(
  attachment: ChatAttachment | null | undefined,
): attachment is ChatAttachment {
  return Boolean(
    attachment?.name && attachment.kind === "image" && attachment.dataUrl,
  );
}

function rememberAttachment(
  current: ChatAttachment[],
  attachment: ChatAttachment,
): ChatAttachment[] {
  const next = current.filter((item) => item.path !== attachment.path);
  next.push(attachment);
  return next.slice(-30);
}

function formatBytes(bytes: number): string {
  if (bytes < 1_024) return `${bytes} B`;
  if (bytes < 1_048_576) return `${(bytes / 1_024).toFixed(1)} KB`;
  return `${(bytes / 1_048_576).toFixed(1)} MB`;
}
