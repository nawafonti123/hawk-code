import type { GitStatus } from "@hawk-code/shared-types";
import { ChevronDown, FileCode2, Files, LoaderCircle } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { getWorkspaceGitDiff } from "../lib/ipc";

interface ChangeSummaryCardProps {
  status: GitStatus;
  workspacePath: string;
  compact?: boolean;
}

export function ChangeSummaryCard({
  status,
  workspacePath,
  compact = false,
}: ChangeSummaryCardProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [patch, setPatch] = useState<string | null>(null);
  const [loadingPath, setLoadingPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (status.clean || status.files.length === 0) return null;

  const openFile = async (filePath: string) => {
    if (selectedPath === filePath) {
      setSelectedPath(null);
      setPatch(null);
      setError(null);
      return;
    }
    setSelectedPath(filePath);
    setPatch(null);
    setError(null);
    setLoadingPath(filePath);
    try {
      const result = await getWorkspaceGitDiff(workspacePath, filePath);
      setPatch(result.patch);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setLoadingPath(null);
    }
  };

  return (
    <section
      className={`change-summary${compact ? " change-summary--compact" : ""}`}
      aria-label={t("git.reviewChanges")}
    >
      <button
        type="button"
        className="change-summary__trigger"
        aria-expanded={expanded}
        onClick={() => setExpanded((value) => !value)}
      >
        <Files size={16} />
        <strong>{t("git.filesChanged", { count: status.fileCount })}</strong>
        <span className="diff-additions">+{status.additions}</span>
        <span className="diff-deletions">-{status.deletions}</span>
        <ChevronDown size={15} data-open={expanded} />
      </button>

      {expanded ? (
        <div className="change-summary__body">
          <div className="change-summary__files" role="list">
            {status.files.map((file) => (
              <button
                key={`${file.status}:${file.path}`}
                type="button"
                role="listitem"
                data-active={selectedPath === file.path}
                aria-expanded={selectedPath === file.path}
                onClick={() => void openFile(file.path)}
              >
                <span className="change-file__status">{file.status}</span>
                <FileCode2 size={14} />
                <span className="change-file__path" dir="ltr">
                  {file.path}
                </span>
                <span className="diff-additions">+{file.additions}</span>
                <span className="diff-deletions">-{file.deletions}</span>
              </button>
            ))}
          </div>

          {selectedPath ? (
            <div className="change-summary__diff" aria-live="polite">
              <div className="change-summary__diff-header">
                <span dir="ltr">{selectedPath}</span>
                {loadingPath ? (
                  <LoaderCircle className="spin" size={14} />
                ) : null}
              </div>
              {error ? <p className="change-summary__error">{error}</p> : null}
              {patch ? (
                <pre dir="ltr" tabIndex={0}>
                  {patch.split("\n").map((line, index) => (
                    <code
                      key={`${index}:${line.slice(0, 24)}`}
                      data-kind={diffLineKind(line)}
                    >
                      {line || " "}
                    </code>
                  ))}
                </pre>
              ) : null}
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

function diffLineKind(line: string): "add" | "delete" | "meta" | "context" {
  if (line.startsWith("+") && !line.startsWith("+++")) return "add";
  if (line.startsWith("-") && !line.startsWith("---")) return "delete";
  if (
    line.startsWith("@@") ||
    line.startsWith("diff ") ||
    line.startsWith("index ") ||
    line.startsWith("---") ||
    line.startsWith("+++")
  )
    return "meta";
  return "context";
}
