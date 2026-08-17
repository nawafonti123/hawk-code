import { Check, Copy, ImageIcon, Pencil, Save, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

export function MessageResponse({
  children,
  onPromptSave,
}: {
  children: string;
  onPromptSave?: (previous: string, next: string) => void;
}) {
  return (
    <div className="message-response">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents(onPromptSave)}>
        {children}
      </ReactMarkdown>
    </div>
  );
}

function markdownComponents(
  onPromptSave?: (previous: string, next: string) => void,
): Components {
  return {
    pre: ({ children }) => <>{children}</>,
    code: ({ className, children, ...props }) => {
      const value = textFromChildren(children).replace(/\n$/u, "");
      const language = /language-([\w-]+)/u.exec(className ?? "")?.[1];
      const block = Boolean(language) || value.includes("\n");
      if (language === "prompt")
        return onPromptSave ? (
          <PromptBlock key={value} prompt={value} onSave={onPromptSave} />
        ) : (
          <PromptBlock key={value} prompt={value} />
        );
      if (!block) {
        const generatedImage = generatedImageName(value);
        if (generatedImage) {
          return (
            <button
              type="button"
              className="generated-attachment-link"
              data-hawk-generated-attachment={generatedImage}
              title="فتح الصورة داخل HAWK Code"
            >
              <ImageIcon size={13} aria-hidden="true" />
              <code className={className} {...props}>
                {children}
              </code>
            </button>
          );
        }
      }
      return block ? (
        <CodeBlock code={value} language={language ?? "text"} />
      ) : (
        <code className={className} {...props}>
          {children}
        </code>
      );
    },
    a: ({ href, children }) => (
      <a href={href} target="_blank" rel="noreferrer">
        {children}
      </a>
    ),
  };
}

function generatedImageName(value: string): string | null {
  const normalized = value.trim().replace(/\\/gu, "/");
  const filename = normalized.split("/").pop()?.trim() ?? "";
  if (!/^playwright-cli.*\.(?:png|jpe?g|webp)$/iu.test(normalized) &&
      !/^page-.*\.(?:png|jpe?g|webp)$/iu.test(filename)) {
    return null;
  }
  return filename || null;
}

function textFromChildren(children: React.ReactNode): string {
  if (typeof children === "string") return children;
  if (typeof children === "number") return `${children}`;
  if (Array.isArray(children)) return children.map(textFromChildren).join("");
  return "";
}

function PromptBlock({
  prompt,
  onSave,
}: {
  prompt: string;
  onSave?: (previous: string, next: string) => void;
}) {
  const { t } = useTranslation();
  const [value, setValue] = useState(prompt);
  const [editing, setEditing] = useState(false);
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1_600);
  };
  return (
    <section className="prompt-block" aria-label="Prompt">
      <header>
        <span>PROMPT</span>
        <div>
          <button type="button" onClick={() => void copy()}>
            {copied ? <Check size={14} /> : <Copy size={14} />}
            <span>{copied ? t("message.copied") : t("message.copy")}</span>
          </button>
          <button
            type="button"
            onClick={() => {
              if (editing) setValue(prompt);
              setEditing((current) => !current);
            }}
          >
            {editing ? <X size={14} /> : <Pencil size={14} />}
            <span>{editing ? t("prompt.cancel") : t("prompt.edit")}</span>
          </button>
          {editing ? (
            <button
              type="button"
              onClick={() => {
                onSave?.(prompt, value);
                setEditing(false);
              }}
            >
              <Save size={14} />
              <span>{t("prompt.save")}</span>
            </button>
          ) : null}
        </div>
      </header>
      {editing ? (
        <textarea value={value} onChange={(event) => setValue(event.target.value)} />
      ) : (
        <pre dir="auto">{value}</pre>
      )}
    </section>
  );
}

function CodeBlock({ code, language }: { code: string; language: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1_600);
  };
  return (
    <div className="code-block">
      <div className="code-block__header">
        <span>{language}</span>
        <button
          type="button"
          onClick={() => void copy()}
          aria-label={t("message.copyCode")}
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
          <span>
            {copied ? t("message.codeCopied") : t("message.copyCode")}
          </span>
        </button>
      </div>
      <pre dir="ltr">
        <code>{code}</code>
      </pre>
    </div>
  );
}
