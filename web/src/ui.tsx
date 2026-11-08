import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useI18n } from "./i18n";
import type { ActivityEntry } from "./types";

/* ==========================================================================
   1. Spinner Primitive
   ========================================================================== */

export function Spinner({
  size = 14,
  tone = "current",
}: {
  size?: number;
  tone?: "current" | "amber" | "green" | "muted" | "dark";
}) {
  return (
    <svg
      className={`ui-spinner tone-${tone}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
    >
      <circle
        className="ui-spinner-track"
        cx="12"
        cy="12"
        r="9"
        stroke="currentColor"
        strokeWidth="2.6"
      />
      <path
        className="ui-spinner-head"
        d="M21 12a9 9 0 0 0-9-9"
        stroke="currentColor"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
    </svg>
  );
}

/* ==========================================================================
   2. CopyButton with Spring Pop Micro-Interaction
   ========================================================================== */

export function CopyButton({
  text,
  label = "Copy",
  copiedLabel = "✓ Copied",
  className = "btn-ghost compact",
  onCopied,
}: {
  text: string;
  label?: ReactNode;
  copiedLabel?: ReactNode;
  className?: string;
  onCopied?: () => void;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const id = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(id);
  }, [copied]);

  return (
    <button
      type="button"
      className={`${className} copy-btn ${copied ? "is-copied" : ""}`}
      onClick={(e) => {
        e.stopPropagation();
        navigator.clipboard?.writeText(text);
        setCopied(true);
        onCopied?.();
      }}
    >
      <span className="copy-btn-inner">{copied ? copiedLabel : label}</span>
    </button>
  );
}

/* ==========================================================================
   3. Elapsed Live Timer Hook (for active network/RPC operations)
   ========================================================================== */

export function useElapsedTimer(active: boolean): number {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (!active) {
      setElapsed(0);
      return;
    }
    const start = performance.now();
    const timer = window.setInterval(() => {
      setElapsed(Math.round(performance.now() - start));
    }, 45);
    return () => window.clearInterval(timer);
  }, [active]);
  return elapsed;
}

/* ==========================================================================
   4. Pretty JSON / YAML / Raw CodeViewer with Syntax Highlighting
   ========================================================================== */

export type DataFormatMode = "json" | "yaml" | "raw";

export function CodeViewer({
  data,
  rawText,
  defaultMode = "json",
  title,
  statusPill,
  durationMs,
  loading = false,
  loadingLabel = "Executing MCP request…",
  maxHeight = 420,
  onNotify,
}: {
  data?: unknown;
  rawText?: string;
  defaultMode?: DataFormatMode;
  title?: ReactNode;
  statusPill?: ReactNode;
  durationMs?: number | null;
  loading?: boolean;
  loadingLabel?: string;
  maxHeight?: number;
  onNotify?: (msg: string) => void;
}) {
  const { t } = useI18n();
  const [mode, setMode] = useState<DataFormatMode>(defaultMode);
  const elapsed = useElapsedTimer(loading);

  const normalized = useMemo(() => {
    if (data !== undefined && data !== null) {
      return unwrapMcpPayload(data);
    }
    if (rawText) {
      try {
        const parsed = JSON.parse(rawText);
        return unwrapMcpPayload(parsed);
      } catch {
        return { parsed: rawText, unwrapped: false };
      }
    }
    return { parsed: null, unwrapped: false };
  }, [data, rawText]);

  const formattedJson = useMemo(() => {
    if (normalized.parsed === null || normalized.parsed === undefined) {
      return rawText ?? "";
    }
    if (typeof normalized.parsed === "string") {
      return normalized.parsed;
    }
    return JSON.stringify(normalized.parsed, null, 2);
  }, [normalized, rawText]);

  const formattedYaml = useMemo(() => {
    if (normalized.parsed === null || normalized.parsed === undefined) {
      return rawText ?? "";
    }
    return jsonToYaml(normalized.parsed).trimEnd();
  }, [normalized, rawText]);

  const rawOutput = useMemo(() => {
    if (rawText !== undefined) return rawText;
    if (typeof data === "string") return data;
    if (data !== undefined) return JSON.stringify(data, null, 2);
    return "";
  }, [data, rawText]);

  const activeText =
    mode === "json" ? formattedJson : mode === "yaml" ? formattedYaml : rawOutput;

  return (
    <div className="code-viewer">
      <div className="code-viewer-toolbar">
        <div className="row" style={{ gap: 8 }}>
          <div className="segmented-tabs">
            <button
              type="button"
              className={mode === "json" ? "active" : ""}
              onClick={() => setMode("json")}
            >
              {t("viewer.prettyJson")}
            </button>
            <button
              type="button"
              className={mode === "yaml" ? "active" : ""}
              onClick={() => setMode("yaml")}
            >
              {t("viewer.yaml")}
            </button>
            <button
              type="button"
              className={mode === "raw" ? "active" : ""}
              onClick={() => setMode("raw")}
            >
              {t("viewer.raw")}
            </button>
          </div>
          {normalized.unwrapped && mode !== "raw" ? (
            <span className="badge-tag subtle" title="Inner JSON inside MCP text content was automatically parsed">
              {t("viewer.autoUnwrapped")}
            </span>
          ) : null}
          {title}
          {statusPill}
          {durationMs !== undefined && durationMs !== null ? (
            <span className="timing-badge">{durationMs} ms</span>
          ) : null}
        </div>

        <div className="row" style={{ gap: 8 }}>
          {activeText ? (
            <CopyButton
              text={activeText}
              label={`Copy ${mode.toUpperCase()}`}
              copiedLabel={`✓ Copied ${mode.toUpperCase()}`}
              onCopied={() => onNotify?.(`Copied ${mode.toUpperCase()} to clipboard`)}
            />
          ) : null}
        </div>
      </div>

      <div className="code-viewer-body" style={{ maxHeight }}>
        {loading ? (
          <div className="code-viewer-loading-overlay">
            <Spinner size={20} tone="amber" />
            <span>{loadingLabel}</span>
            <code className="elapsed-pill">{elapsed} ms</code>
          </div>
        ) : null}
        <pre key={mode} className="code-viewer-pre mode-switch-anim">
          {mode === "json"
            ? highlightJson(formattedJson)
            : mode === "yaml"
              ? highlightYaml(formattedYaml)
              : rawOutput}
        </pre>
      </div>
    </div>
  );
}

/**
 * Unwraps MCP `{ content: [{ type: "text", text: "{\"a\":1}" }] }` payloads
 * so Pretty JSON and YAML views show structured inner objects cleanly.
 */
function unwrapMcpPayload(input: unknown): { parsed: unknown; unwrapped: boolean } {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    return { parsed: input, unwrapped: false };
  }
  const obj = input as Record<string, unknown>;
  if (!Array.isArray(obj.content)) {
    return { parsed: input, unwrapped: false };
  }
  let didUnwrap = false;
  const unwrappedContent = obj.content.map((item) => {
    if (!item || typeof item !== "object") return item;
    const entry = { ...(item as Record<string, unknown>) };
    if (typeof entry.text === "string") {
      const trimmed = entry.text.trim();
      if (
        (trimmed.startsWith("{") && trimmed.endsWith("}")) ||
        (trimmed.startsWith("[") && trimmed.endsWith("]"))
      ) {
        try {
          entry.data = JSON.parse(trimmed);
          delete entry.text;
          didUnwrap = true;
        } catch {
          // keep text as is
        }
      }
    }
    return entry;
  });
  if (didUnwrap) {
    return {
      parsed: {
        ...obj,
        content: unwrappedContent,
      },
      unwrapped: true,
    };
  }
  return { parsed: input, unwrapped: false };
}

export function jsonToYaml(value: unknown, indent = 0): string {
  const pad = "  ".repeat(indent);
  if (value === null || value === undefined) {
    return "null";
  }
  if (typeof value === "boolean" || typeof value === "number") {
    return String(value);
  }
  if (typeof value === "string") {
    if (value.includes("\n")) {
      const lines = value
        .split("\n")
        .map((line) => `${"  ".repeat(indent + 1)}${line}`)
        .join("\n");
      return `|-\n${lines}`;
    }
    if (
      value === "" ||
      /[:#{}[\],&*!|>'"%@`]/.test(value) ||
      /^\s|\s$/.test(value) ||
      /^(true|false|null|yes|no|\d+(\.\d+)?)$/i.test(value)
    ) {
      return JSON.stringify(value);
    }
    return value;
  }
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    return value
      .map((item) => {
        if (item && typeof item === "object" && !Array.isArray(item)) {
          const objLines = jsonToYaml(item, indent + 1).split("\n");
          const first = objLines[0]?.trimStart() ?? "";
          const rest = objLines.slice(1).join("\n");
          return rest ? `${pad}- ${first}\n${rest}` : `${pad}- ${first}`;
        }
        return `${pad}- ${jsonToYaml(item, indent + 1)}`;
      })
      .join("\n");
  }
  if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) return "{}";
    return entries
      .map(([k, v]) => {
        const safeKey = /^[a-zA-Z0-9_-]+$/.test(k) ? k : JSON.stringify(k);
        if (
          v &&
          typeof v === "object" &&
          ((Array.isArray(v) && v.length > 0) ||
            (!Array.isArray(v) && Object.keys(v).length > 0))
        ) {
          return `${pad}${safeKey}:\n${jsonToYaml(v, indent + 1)}`;
        }
        return `${pad}${safeKey}: ${jsonToYaml(v, indent)}`;
      })
      .join("\n");
  }
  return String(value);
}

const JSON_TOKEN_REGEX =
  /("(?:\\u[a-fA-F0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(?:true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+-]?\d+)?|[{}\[\],:])/g;

export function highlightJson(text: string): ReactNode {
  if (!text) return null;
  const parts: ReactNode[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let key = 0;
  JSON_TOKEN_REGEX.lastIndex = 0;

  while ((match = JSON_TOKEN_REGEX.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    if (token.startsWith('"')) {
      if (token.endsWith(":")) {
        const colonIdx = token.lastIndexOf(":");
        const keyPart = token.slice(0, colonIdx);
        const colonPart = token.slice(colonIdx);
        parts.push(
          <span key={key++} className="syn-key">
            {keyPart}
          </span>,
        );
        parts.push(
          <span key={key++} className="syn-punct">
            {colonPart}
          </span>,
        );
      } else {
        parts.push(
          <span key={key++} className="syn-string">
            {token}
          </span>,
        );
      }
    } else if (token === "true" || token === "false") {
      parts.push(
        <span key={key++} className="syn-boolean">
          {token}
        </span>,
      );
    } else if (token === "null") {
      parts.push(
        <span key={key++} className="syn-null">
          {token}
        </span>,
      );
    } else if (/^[{}\[\],:]$/.test(token)) {
      parts.push(
        <span key={key++} className="syn-punct">
          {token}
        </span>,
      );
    } else {
      parts.push(
        <span key={key++} className="syn-number">
          {token}
        </span>,
      );
    }
    lastIndex = JSON_TOKEN_REGEX.lastIndex;
  }
  if (lastIndex < text.length) {
    parts.push(text.slice(lastIndex));
  }
  return parts;
}

export function highlightYaml(text: string): ReactNode {
  if (!text) return null;
  const lines = text.split("\n");
  return lines.map((line, idx) => {
    const nl = idx < lines.length - 1 ? "\n" : "";
    const keyMatch = /^(\s*(?:-\s+)?)([a-zA-Z0-9_."-]+)(:)(\s*)(.*)$/.exec(line);
    if (keyMatch) {
      const [, prefix, k, colon, space, rest] = keyMatch;
      return (
        <span key={idx}>
          <span className="syn-punct">{prefix}</span>
          <span className="syn-key">{k}</span>
          <span className="syn-punct">{colon}</span>
          {space}
          {styleYamlScalar(rest)}
          {nl}
        </span>
      );
    }
    const listMatch = /^(\s*-\s+)(.*)$/.exec(line);
    if (listMatch) {
      const [, bullet, rest] = listMatch;
      return (
        <span key={idx}>
          <span className="syn-punct">{bullet}</span>
          {styleYamlScalar(rest)}
          {nl}
        </span>
      );
    }
    return (
      <span key={idx}>
        <span className="syn-string">{line}</span>
        {nl}
      </span>
    );
  });
}

function styleYamlScalar(raw: string): ReactNode {
  if (!raw) return null;
  if (raw === "true" || raw === "false") {
    return <span className="syn-boolean">{raw}</span>;
  }
  if (raw === "null" || raw === "~") {
    return <span className="syn-null">{raw}</span>;
  }
  if (/^-?\d+(\.\d+)?$/.test(raw)) {
    return <span className="syn-number">{raw}</span>;
  }
  if (raw === "[]" || raw === "{}" || raw === "|-") {
    return <span className="syn-punct">{raw}</span>;
  }
  return <span className="syn-string">{raw}</span>;
}

/* ==========================================================================
   5. Live Terminal Gadget (macOS Warp / Inspector Style)
   ========================================================================== */

export function LiveTerminalGadget({
  title = "loopback-mcp-inspector — live telemetry stream",
  entries,
  filterSlug,
  busy = false,
  busyText = "Running MCP operation…",
  defaultExpanded = true,
  maxHeight = 260,
  onClear,
  onNotify,
}: {
  title?: string;
  entries: ActivityEntry[];
  filterSlug?: string;
  busy?: boolean;
  busyText?: string;
  defaultExpanded?: boolean;
  maxHeight?: number;
  onClear?: () => void;
  onNotify?: (msg: string) => void;
}) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [autoScroll, setAutoScroll] = useState(true);
  const [selectedSeq, setSelectedSeq] = useState<number | null>(null);
  const [sourceFilter, setSourceFilter] = useState<string>("all");
  const [errorsOnly, setErrorsOnly] = useState<boolean>(false);
  const [searchQuery, setSearchQuery] = useState<string>("");
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const elapsed = useElapsedTimer(busy);

  const visibleEntries = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    return entries.filter((e) => {
      if (filterSlug && e.slug !== filterSlug && e.slug !== "all") {
        return false;
      }
      if (sourceFilter === "policy") {
        if (e.kind !== "policy") return false;
      } else if (sourceFilter !== "all" && e.source !== sourceFilter) {
        return false;
      }
      if (errorsOnly && e.ok && e.phase !== "error") {
        return false;
      }
      if (q) {
        const hay =
          `${e.slug} ${e.method} ${e.tool ?? ""} ${e.message} ${e.requestPreview ?? ""} ${e.responsePreview ?? ""}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }, [entries, filterSlug, sourceFilter, errorsOnly, searchQuery]);

  useEffect(() => {
    if (autoScroll && expanded && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
    }
  }, [visibleEntries.length, busy, autoScroll, expanded]);

  const plainLogText = useMemo(() => {
    return visibleEntries
      .map(
        (e) =>
          `[${formatShortTime(e.timestamp)}] [${e.source.toUpperCase()}] [${e.slug}] ${e.method} (${e.phase}) ${e.message}${e.durationMs !== undefined && e.durationMs !== null ? ` +${e.durationMs}ms` : ""}`,
      )
      .join("\n");
  }, [visibleEntries]);

  return (
    <div className={`terminal-gadget ${busy ? "is-busy" : ""}`}>
      <div
        className="terminal-bar"
        onClick={() => setExpanded((prev) => !prev)}
      >
        <div className="terminal-bar-left">
          <div className="traffic-lights" aria-hidden="true">
            <span className="tl red" />
            <span className="tl yellow" />
            <span className="tl green" />
          </div>
          <span className="terminal-title">{title}</span>
          {busy ? (
            <span className="terminal-live-status busy">
              <Spinner size={12} tone="amber" />
              <span>{busyText}</span>
              <code>{elapsed}ms</code>
            </span>
          ) : (
            <span className="terminal-live-status idle">
              <i className="lamp on" />
              <span>STREAMING ({visibleEntries.length})</span>
            </span>
          )}
        </div>

        <div
          className="terminal-bar-right"
          onClick={(e) => e.stopPropagation()}
        >
          <input
            type="text"
            className="terminal-filter-select"
            style={{ width: 135, padding: "2px 8px" }}
            placeholder={t("terminal.searchPlaceholder")}
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          <select
            className="terminal-filter-select"
            value={sourceFilter}
            onChange={(e) => setSourceFilter(e.target.value)}
            title="Filter by event source"
          >
            <option value="all">{t("terminal.allSources")}</option>
            <option value="mcp">{t("terminal.mcpCalls")}</option>
            <option value="admin">{t("terminal.admin")}</option>
            <option value="probe">{t("terminal.probe")}</option>
            <option value="policy">{t("terminal.policyGuard")}</option>
            <option value="system">{t("terminal.system")}</option>
          </select>

          <button
            type="button"
            className={`terminal-btn ${errorsOnly ? "active" : ""}`}
            onClick={() => setErrorsOnly((p) => !p)}
            title="Show only errors and blocked calls"
          >
            {t("terminal.errorsOnly")}
          </button>

          <button
            type="button"
            className={`terminal-btn ${autoScroll ? "active" : ""}`}
            onClick={() => setAutoScroll((p) => !p)}
            title="Toggle auto-scroll"
          >
            {t("terminal.autoScroll")}
          </button>

          {plainLogText ? (
            <CopyButton
              text={plainLogText}
              label={t("terminal.copyLog")}
              copiedLabel={t("common.copied")}
              className="terminal-btn"
              onCopied={() => onNotify?.("Copied terminal log")}
            />
          ) : null}

          {onClear && visibleEntries.length > 0 ? (
            <button
              type="button"
              className="terminal-btn"
              onClick={onClear}
            >
              {t("terminal.clear")}
            </button>
          ) : null}

          <button
            type="button"
            className="terminal-btn"
            onClick={() => setExpanded((p) => !p)}
          >
            {expanded ? "▾ Hide" : "▸ Show"}
          </button>
        </div>
      </div>

      <div className={`collapsible-drawer ${expanded ? "open" : ""}`}>
        <div className="collapsible-inner">
          <div
            ref={bodyRef}
            className="terminal-body"
            style={{ maxHeight }}
          >
            {visibleEntries.length === 0 && !busy ? (
              <div className="terminal-empty">
                <span>{t("terminal.emptyPrompt")}</span>
                <small>
                  Every <code>initialize</code>, <code>tools/list</code>,{" "}
                  <code>tools/call</code>, and OAuth token refresh streams here in real time.
                </small>
              </div>
            ) : null}

            {visibleEntries.map((entry) => {
              const hasPayload = Boolean(
                entry.requestPreview || entry.responsePreview,
              );
              const isSelected = selectedSeq === entry.seq;
              return (
                <div key={entry.seq} className="terminal-entry-wrap">
                  <div
                    className={`terminal-line phase-${entry.phase} ${hasPayload ? "clickable" : ""}`}
                    onClick={() => {
                      if (hasPayload) {
                        setSelectedSeq(isSelected ? null : entry.seq);
                      }
                    }}
                  >
                    <span className="term-time">
                      {formatShortTime(entry.timestamp)}
                    </span>
                    <span className={`term-badge src-${entry.source}`}>
                      {entry.source.toUpperCase()}
                    </span>
                    <span className="term-slug">[{entry.slug}]</span>
                    <span className={`term-phase phase-${entry.phase}`}>
                      {entry.phase === "start"
                        ? "→ RUN"
                        : entry.phase === "ok"
                          ? "✓ OK"
                          : "✗ ERR"}
                    </span>
                    <span className="term-method">{entry.method}</span>
                    <span className="term-msg">{entry.message}</span>
                    {entry.durationMs !== undefined &&
                    entry.durationMs !== null ? (
                      <span className="term-dur">+{entry.durationMs}ms</span>
                    ) : null}
                    {hasPayload ? (
                      <span className="term-expand-hint">
                        {isSelected ? "▾ payload" : "▸ payload"}
                      </span>
                    ) : null}
                  </div>

                  {isSelected && hasPayload ? (
                    <div className="terminal-payload-drawer">
                      {entry.requestPreview ? (
                        <div className="term-payload-block">
                          <div className="term-payload-label">
                            Request Arguments (Redacted)
                          </div>
                          <CodeViewer
                            rawText={entry.requestPreview}
                            maxHeight={160}
                            onNotify={onNotify}
                          />
                        </div>
                      ) : null}
                      {entry.responsePreview ? (
                        <div className="term-payload-block">
                          <div className="term-payload-label">
                            Response Payload Preview
                          </div>
                          <CodeViewer
                            rawText={entry.responsePreview}
                            maxHeight={200}
                            onNotify={onNotify}
                          />
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              );
            })}

            {busy ? (
              <div className="terminal-line phase-start active-pending-line">
                <span className="term-time">{formatShortTime(new Date().toISOString())}</span>
                <span className="term-badge src-probe">EXEC</span>
                <span className="term-phase phase-start">
                  <Spinner size={11} tone="amber" /> WAIT
                </span>
                <span className="term-msg">{busyText}</span>
                <span className="term-dur">{elapsed}ms…</span>
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}

function formatShortTime(iso: string): string {
  try {
    const d = new Date(iso);
    const hh = String(d.getHours()).padStart(2, "0");
    const mm = String(d.getMinutes()).padStart(2, "0");
    const ss = String(d.getSeconds()).padStart(2, "0");
    return `${hh}:${mm}:${ss}`;
  } catch {
    return iso.slice(11, 19);
  }
}
