import { FileText, ImageIcon, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ChatAttachment } from "@hawk-code/shared-types";
import { useWorkbenchStore } from "../store/workbench";

export function AttachmentPreviewHost() {
  const messages = useWorkbenchStore((state) => state.messages);
  const composerAttachments = useWorkbenchStore((state) => state.attachments);
  const [preview, setPreview] = useState<ChatAttachment | null>(null);

  const allAttachments = useMemo(
    () => [
      ...composerAttachments,
      ...messages.flatMap((message) => message.attachments ?? []),
    ],
    [composerAttachments, messages],
  );

  useEffect(() => {
    const handleClick = (event: MouseEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
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
  }, [allAttachments]);

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
            aria-label="Close preview"
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

function formatBytes(bytes: number): string {
  if (bytes < 1_024) return `${bytes} B`;
  if (bytes < 1_048_576) return `${(bytes / 1_024).toFixed(1)} KB`;
  return `${(bytes / 1_048_576).toFixed(1)} MB`;
}
