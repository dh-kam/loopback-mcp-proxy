import { type ReactNode, useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { useI18n } from "./i18n";
import { RequiredAlertIcon, RequiredCheckIcon } from "./icons";
import type {
  JsonSchema,
  ProbeCallResult,
  ServerInput,
  ServerView,
  ToolDef,
  ToolPolicy,
} from "./types";
import {
  CodeViewer,
  CopyButton,
  highlightJson,
  Spinner,
  useElapsedTimer,
} from "./ui";

const DESTRUCTIVE_VERBS = new Set([
  "delete",
  "remove",
  "drop",
  "destroy",
  "purge",
  "truncate",
  "revoke",
  "wipe",
  "kill",
  "terminate",
  "refund",
  "ban",
  "unpublish",
  "erase",
  "reset",
]);

const WRITE_VERBS = new Set([
  "create",
  "update",
  "upsert",
  "insert",
  "add",
  "set",
  "write",
  "put",
  "post",
  "patch",
  "edit",
  "modify",
  "send",
  "publish",
  "merge",
  "close",
  "reopen",
  "assign",
  "move",
  "rename",
  "copy",
  "duplicate",
  "execute",
  "exec",
  "run",
  "trigger",
  "start",
  "stop",
  "restart",
  "deploy",
  "apply",
  "commit",
  "push",
  "transfer",
  "approve",
  "reject",
  "comment",
  "reply",
  "upload",
  "import",
  "archive",
  "cancel",
  "clear",
]);

const READ_VERBS = new Set([
  "get",
  "list",
  "search",
  "read",
  "fetch",
  "find",
  "query",
  "inspect",
  "describe",
  "show",
  "view",
  "check",
  "verify",
  "count",
  "ping",
  "whoami",
  "me",
  "status",
  "info",
  "lookup",
  "resolve",
  "retrieve",
  "browse",
  "export",
  "download",
]);

export function classifyToolRisk(
  tool: ToolDef,
): "read" | "write" | "destructive" {
  const annotations = (tool as Record<string, unknown>).annotations as
    | Record<string, unknown>
    | undefined;
  if (annotations?.destructiveHint === true) {
    return "destructive";
  }
  if (annotations?.readOnlyHint === true) {
    return "read";
  }
  const tokens = tool.name
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
  if (tokens.some((t) => DESTRUCTIVE_VERBS.has(t))) return "destructive";
  if (tokens.some((t) => WRITE_VERBS.has(t))) return "write";
  if (tokens.some((t) => READ_VERBS.has(t))) return "read";
  if (annotations?.readOnlyHint === false) return "write";
  return "read";
}

function matchesPatternList(list: string[], name: string): boolean {
  return list.some((pattern) => {
    const p = pattern.trim();
    if (!p) return false;
    if (p === "*") return true;
    if (p.endsWith("*")) return name.startsWith(p.slice(0, -1));
    return p === name;
  });
}

export function isToolAllowedByPolicy(
  tool: ToolDef,
  policy?: ToolPolicy | null,
): boolean {
  if (!policy) return true;
  const clean = tool.name.trim();
  if (matchesPatternList(policy.deniedTools ?? [], clean)) {
    return false;
  }
  if (policy.mode === "allowAll" || policy.mode === "denylist") {
    return true;
  }
  if (policy.mode === "allowlist") {
    return matchesPatternList(policy.allowedTools ?? [], clean);
  }
  if (policy.mode === "readOnly") {
    return (
      matchesPatternList(policy.allowedTools ?? [], clean) ||
      classifyToolRisk(tool) === "read"
    );
  }
  return true;
}

interface McpContentItem {
  type?: string;
  text?: string;
  mimeType?: string;
  data?: unknown;
  [key: string]: unknown;
}

export interface McpToolResponse {
  content?: McpContentItem[];
  isError?: boolean;
  [key: string]: unknown;
}

export function ToolBench({
  server,
  tools: propTools,
  slug: propSlug,
  probeInput,
  initialToolName,
  compact = false,
  onNotify,
}: {
  server?: ServerView;
  tools?: ToolDef[];
  slug?: string;
  probeInput?: ServerInput;
  initialToolName?: string;
  compact?: boolean;
  onNotify?: (msg: string) => void;
}) {
  const { t } = useI18n();
  const tools = propTools ?? server?.tools ?? [];
  const slug = propSlug ?? server?.slug ?? "upstream";

  const [filter, setFilter] = useState("");
  const [name, setName] = useState(
    initialToolName || tools[0]?.name || "",
  );

  useEffect(() => {
    if (initialToolName && tools.some((t) => t.name === initialToolName)) {
      setName(initialToolName);
    }
  }, [initialToolName, tools]);

  const filteredTools = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return tools;
    return tools.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        (t.description ?? "").toLowerCase().includes(q),
    );
  }, [tools, filter]);

  const tool =
    tools.find((item) => item.name === name) ??
    filteredTools[0] ??
    tools[0];

  const [rawMode, setRawMode] = useState(false);
  const [raw, setRaw] = useState("{}");
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [responseObj, setResponseObj] = useState<McpToolResponse | null>(null);
  const [rawResult, setRawResult] = useState<string>("");
  const [durationMs, setDurationMs] = useState<number | null>(null);
  const [error, setError] = useState("");
  const [running, setRunning] = useState(false);
  const [savingPolicy, setSavingPolicy] = useState(false);
  const elapsed = useElapsedTimer(running);
  const policy: ToolPolicy = server?.toolPolicy ?? {
    mode: "allowAll",
    allowedTools: [],
    deniedTools: [],
  };

  async function savePolicy(nextPolicy: ToolPolicy, summaryMsg: string) {
    if (!server) return;
    setSavingPolicy(true);
    try {
      await api<ServerView>(`/api/servers/${server.id}/policy`, {
        method: "PUT",
        body: JSON.stringify(nextPolicy),
      });
      onNotify?.(summaryMsg);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update tool policy");
    } finally {
      setSavingPolicy(false);
    }
  }

  function toggleSingleTool(target: ToolDef) {
    const currentlyAllowed = isToolAllowedByPolicy(target, policy);
    const cleanName = target.name.trim();
    const nextDenied = new Set(policy.deniedTools ?? []);
    const nextAllowed = new Set(policy.allowedTools ?? []);
    if (currentlyAllowed) {
      nextDenied.add(cleanName);
      nextAllowed.delete(cleanName);
    } else {
      nextDenied.delete(cleanName);
      if (policy.mode === "allowlist" || policy.mode === "readOnly") {
        nextAllowed.add(cleanName);
      }
    }
    const nextMode =
      policy.mode === "allowAll" && nextDenied.size > 0
        ? "denylist"
        : policy.mode;
    void savePolicy(
      {
        mode: nextMode,
        allowedTools: Array.from(nextAllowed),
        deniedTools: Array.from(nextDenied),
      },
      `${currentlyAllowed ? "Blocked" : "Allowed"} tool "${cleanName}" on /mcp/${slug}`,
    );
  }

  useEffect(() => {
    if (!tools.some((item) => item.name === name)) {
      setName(tools[0]?.name ?? "");
    }
  }, [tools, name]);

  useEffect(() => {
    const next = defaultsFrom(tool?.inputSchema);
    setValues(next);
    setRaw(JSON.stringify(next, null, 2));
    setResponseObj(null);
    setRawResult("");
    setDurationMs(null);
    setError("");
    setShowMissingGuard(false);
  }, [tool?.name]);

  const [showMissingGuard, setShowMissingGuard] = useState(false);
  const [shakeTick, setShakeTick] = useState(0);

  const schema = tool?.inputSchema;
  const simple = useMemo(() => isSimpleObject(schema), [schema]);
  const propCount = useMemo(
    () => Object.keys(schema?.properties ?? {}).length,
    [schema],
  );
  const totalRequiredCount = schema?.required?.length ?? 0;

  const effectiveArgs = useMemo<Record<string, unknown>>(() => {
    if (rawMode || !simple) {
      try {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
          return parsed as Record<string, unknown>;
        }
        return {};
      } catch {
        return values;
      }
    }
    return values;
  }, [rawMode, simple, raw, values]);

  const missingRequired = useMemo(
    () => getMissingRequiredFields(schema, effectiveArgs),
    [schema, effectiveArgs],
  );

  useEffect(() => {
    if (missingRequired.length === 0 && showMissingGuard) {
      setShowMissingGuard(false);
    }
  }, [missingRequired.length, showMissingGuard]);

  function focusFieldByKey(fieldKey: string) {
    const rootKey = fieldKey.split(".")[0];
    if (rawMode && simple) {
      handleModeSwitch(false);
    }
    window.setTimeout(() => {
      if (!simple) {
        const rawEditor = document.querySelector<HTMLTextAreaElement>(
          `.workbench-main .code-editor`,
        );
        rawEditor?.scrollIntoView({ behavior: "smooth", block: "center" });
        rawEditor?.focus();
        return;
      }
      const exactCardEl = document.getElementById(`bench-card-${slug}-${fieldKey}`);
      const cardEl = exactCardEl ?? document.getElementById(`bench-card-${slug}-${rootKey}`);
      if (cardEl) {
        cardEl.scrollIntoView({ behavior: "smooth", block: "center" });
        const focusable = cardEl.querySelector<HTMLElement>(
          "input:not([type='checkbox']), select, textarea, button, input[type='checkbox']",
        );
        focusable?.focus();
      }
    }, 35);
  }

  function fillMissingRequiredSample() {
    if (!schema) return;
    const fullSample = generateSampleFromSchema(schema);
    const next: Record<string, unknown> = { ...effectiveArgs };
    for (const reqKey of missingRequired) {
      const rootKey = reqKey.split(".")[0];
      if (fullSample[rootKey] !== undefined) {
        next[rootKey] = fullSample[rootKey];
      }
    }
    setValues(next);
    setRaw(JSON.stringify(next, null, 2));
    setShowMissingGuard(false);
    setError("");
    onNotify?.(
      `Filled sample values for missing required field${missingRequired.length > 1 ? "s" : ""}: ${missingRequired.join(", ")}`,
    );
  }

  function updateFormValues(next: Record<string, unknown>) {
    setValues(next);
    setRaw(JSON.stringify(next, null, 2));
  }

  function handleModeSwitch(toRaw: boolean) {
    if (!toRaw && rawMode) {
      try {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
          setValues(parsed as Record<string, unknown>);
        }
      } catch {
        // Keep previous structured values if raw JSON was invalid
      }
    } else if (toRaw && !rawMode) {
      setRaw(JSON.stringify(values, null, 2));
    }
    setRawMode(toRaw);
  }

  function resetDefaults() {
    const next = defaultsFrom(tool?.inputSchema);
    setValues(next);
    setRaw(JSON.stringify(next, null, 2));
    setError("");
  }

  function fillSample() {
    const next = generateSampleFromSchema(tool?.inputSchema);
    setValues(next);
    setRaw(JSON.stringify(next, null, 2));
    setError("");
    onNotify?.(`Filled sample parameters for "${tool?.name ?? "tool"}"`);
  }

  const presetStorageKey = `loopback_presets_${slug}_${tool?.name ?? "default"}`;
  const [presets, setPresets] = useState<Array<{ label: string; args: Record<string, unknown> }>>([]);

  useEffect(() => {
    try {
      const saved = localStorage.getItem(presetStorageKey);
      setPresets(saved ? JSON.parse(saved) : []);
    } catch {
      setPresets([]);
    }
  }, [presetStorageKey]);

  function saveCurrentPreset() {
    if (!tool) return;
    const currentArgs = (() => {
      try {
        return rawMode ? (JSON.parse(raw) as Record<string, unknown>) : values;
      } catch {
        return values;
      }
    })();
    const label = `Preset #${presets.length + 1}`;
    const next = [...presets, { label, args: currentArgs }].slice(-6);
    setPresets(next);
    try {
      localStorage.setItem(presetStorageKey, JSON.stringify(next));
    } catch {
      // ignore storage quota errors
    }
    onNotify?.(`Saved ${label} for "${tool.name}"`);
  }

  function applyPreset(args: Record<string, unknown>, label: string) {
    setValues(args);
    setRaw(JSON.stringify(args, null, 2));
    setError("");
    onNotify?.(`Loaded ${label}`);
  }

  function deletePreset(idx: number) {
    const next = presets.filter((_, i) => i !== idx);
    setPresets(next);
    try {
      localStorage.setItem(presetStorageKey, JSON.stringify(next));
    } catch {
      // ignore
    }
  }

  const curlSnippet = useMemo(() => {
    const toolName = tool?.name ?? "tool";
    const parsedArgs = (() => {
      try {
        return JSON.parse(raw);
      } catch {
        return values;
      }
    })();
    const rpcBody = JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name: toolName, arguments: parsedArgs ?? {} },
    });
    const baseUrl = window.location.origin;
    const escaped = rpcBody.replace(/'/g, `'\\''`);
    return `curl -sS -X POST '${baseUrl}/mcp/${slug}' -H 'Content-Type: application/json' -H 'mcp-session-id: <SESSION_ID>' -d '${escaped}'`;
  }, [tool?.name, raw, values, slug]);

  async function run(forceExecute = false) {
    if (!tool) return;
    if (!forceExecute && missingRequired.length > 0) {
      setShowMissingGuard(true);
      setShakeTick((prev) => prev + 1);
      const firstMissing = missingRequired[0];
      focusFieldByKey(firstMissing);
      onNotify?.(
        `⚠ Missing required parameter${missingRequired.length > 1 ? "s" : ""}: ${missingRequired.join(", ")} — focused "${firstMissing}"`,
      );
      return;
    }
    setShowMissingGuard(false);
    setRunning(true);
    setError("");
    const started = performance.now();
    try {
      const args = rawMode || !simple ? JSON.parse(raw) : values;
      if (probeInput) {
        const probeResp = await api<ProbeCallResult>("/api/probe/call", {
          method: "POST",
          body: JSON.stringify({
            ...probeInput,
            toolName: tool.name,
            arguments: args ?? {},
          }),
        });
        setDurationMs(
          probeResp.durationMs ?? Math.round(performance.now() - started),
        );
        if (!probeResp.ok && !probeResp.result) {
          setError(probeResp.message || "Probe tool call failed");
        } else {
          const resPayload = (probeResp.result ?? {}) as McpToolResponse;
          setResponseObj(resPayload);
          setRawResult(JSON.stringify(resPayload, null, 2));
        }
      } else if (server) {
        const payload = await api<McpToolResponse>(
          `/api/servers/${server.id}/tools/call`,
          {
            method: "POST",
            body: JSON.stringify({ name: tool.name, arguments: args ?? {} }),
          },
        );
        setDurationMs(Math.round(performance.now() - started));
        setResponseObj(payload);
        setRawResult(JSON.stringify(payload, null, 2));
      }
    } catch (err) {
      setDurationMs(Math.round(performance.now() - started));
      setError(err instanceof Error ? err.message : "Tool execution failed");
    } finally {
      setRunning(false);
    }
  }

  if (!tools.length) {
    return (
      <div className="empty-bench">
        <strong>No tools reported by this upstream</strong>
        <p className="muted">
          {server?.state === "connected"
            ? "The server connected successfully but returned an empty tools/list."
            : "Connect or authorize this upstream first to inspect and execute its MCP tools."}
        </p>
      </div>
    );
  }

  return (
    <div className={`workbench ${compact ? "compact" : ""}`}>
      {/* Left: Tool Catalog Explorer */}
      <div className="workbench-sidebar">
        {server ? (
          <div className="policy-bar">
            <div className="policy-bar-top">
              <span className="policy-bar-label">
                {t("bench.guardrailLabel", {
                  exposed:
                    server.exposedToolCount ??
                    tools.filter((tItem) => isToolAllowedByPolicy(tItem, policy)).length,
                  total: tools.length,
                })}
              </span>
              {savingPolicy ? <Spinner size={11} tone="amber" /> : null}
            </div>
            <div className="policy-preset-row">
              <button
                type="button"
                className={`chip-btn ${policy.mode === "allowAll" && (policy.deniedTools?.length ?? 0) === 0 ? "active" : ""}`}
                disabled={savingPolicy}
                onClick={() =>
                  void savePolicy(
                    { mode: "allowAll", allowedTools: [], deniedTools: [] },
                    `Enabled all ${tools.length} tools for ${slug}`,
                  )
                }
              >
                {t("bench.allowAll")}
              </button>
              <button
                type="button"
                className={`chip-btn ${policy.mode === "readOnly" ? "active" : ""}`}
                disabled={savingPolicy}
                onClick={() =>
                  void savePolicy(
                    { mode: "readOnly", allowedTools: [], deniedTools: [] },
                    `Activated Read-Only Shield for ${slug}`,
                  )
                }
              >
                {t("bench.readOnlyShield")}
              </button>
              <button
                type="button"
                className="chip-btn"
                disabled={savingPolicy}
                onClick={() => {
                  const destructive = tools
                    .filter((tItem) => classifyToolRisk(tItem) === "destructive")
                    .map((tItem) => tItem.name);
                  void savePolicy(
                    {
                      mode: "denylist",
                      allowedTools: [],
                      deniedTools: destructive,
                    },
                    `Blocked ${destructive.length} destructive tools on ${slug}`,
                  );
                }}
              >
                {t("bench.blockDestructive")}
              </button>
            </div>
          </div>
        ) : null}
        <div className="workbench-search">
          <input
            placeholder={t("bench.searchToolsPlaceholder", { count: tools.length })}
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          />
        </div>
        <div className="tool-list">
          {filteredTools.map((item) => {
            const pCount = Object.keys(item.inputSchema?.properties ?? {}).length;
            const reqCount = item.inputSchema?.required?.length ?? 0;
            const active = tool && item.name === tool.name;
            const risk = classifyToolRisk(item);
            const allowed = isToolAllowedByPolicy(item, policy);
            return (
              <div
                key={item.name}
                className={`tool-item ${active ? "on" : ""} ${!allowed ? "policy-blocked" : ""}`}
                onClick={() => setName(item.name)}
              >
                <div className="tool-item-head">
                  <div className="row" style={{ gap: 6, minWidth: 0 }}>
                    <strong>{item.name}</strong>
                    <span className={`tool-risk-badge ${risk}`}>
                      {risk === "destructive"
                        ? "DESTRUCT"
                        : risk === "write"
                          ? "WRITE"
                          : "READ"}
                    </span>
                  </div>
                  <div className="row" style={{ gap: 6 }}>
                    <span className="tool-param-badge">
                      {pCount === 0
                        ? t("bench.noArgs")
                        : reqCount > 0
                          ? t("bench.argsWithReq", { total: pCount, req: reqCount })
                          : t("bench.argsOnly", { total: pCount })}
                    </span>
                    {server ? (
                      <button
                        type="button"
                        className={`tool-policy-toggle ${allowed ? "allowed" : "blocked"}`}
                        disabled={savingPolicy}
                        title={
                          allowed
                            ? "Exposed to local /mcp CLI agents (click to block)"
                            : "Blocked from local /mcp CLI agents (click to allow)"
                        }
                        onClick={(e) => {
                          e.stopPropagation();
                          toggleSingleTool(item);
                        }}
                      >
                        {allowed ? "ON" : "BLOCKED"}
                      </button>
                    ) : null}
                  </div>
                </div>
                {item.description ? (
                  <div className="tool-item-desc">
                    {stripDescriptionForPreview(item.description)}
                  </div>
                ) : null}
              </div>
            );
          })}
          {!filteredTools.length ? (
            <div className="muted" style={{ padding: 12, fontSize: 13 }}>
              {t("bench.noToolsMatch", { filter })}
            </div>
          ) : null}
        </div>
      </div>

      {/* Right: Tool Inspector & Runner */}
      {tool ? (
        <div key={tool.name} className="workbench-main fade-slide-in">
          <div className="tool-inspector-head">
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="tool-title-row">
                <h3>{tool.name}</h3>
                <CopyButton
                  text={tool.name}
                  label={<code>{tool.name}</code>}
                  copiedLabel={t("common.copied")}
                  className="copy-chip"
                  onCopied={() => onNotify?.(`Copied tool name "${tool.name}"`)}
                />
                <CopyButton
                  text={`${slug}__${tool.name}`}
                  label={
                    <code>
                      {slug}__{tool.name}
                    </code>
                  }
                  copiedLabel={t("common.copied")}
                  className="copy-chip subtle"
                  onCopied={() =>
                    onNotify?.(`Copied namespaced "${slug}__${tool.name}"`)
                  }
                />
              </div>
              {tool.description ? (
                <RichDescription
                  text={tool.description}
                  variant="tool"
                  onApplyArgs={(args, label) => {
                    setValues(args);
                    setRaw(JSON.stringify(args, null, 2));
                    setShowMissingGuard(false);
                    setError("");
                    onNotify?.(
                      `Applied example${label ? ` "${label}"` : ""} to ${tool.name}`,
                    );
                  }}
                  onNotify={onNotify}
                />
              ) : (
                <p className="tool-desc muted">
                  No description provided by upstream.
                </p>
              )}
            </div>
            <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
              {missingRequired.length > 0 && !running ? (
                <button
                  type="button"
                  className="req-missing-hint-pill"
                  onClick={() => {
                    setShowMissingGuard(true);
                    setShakeTick((prev) => prev + 1);
                    focusFieldByKey(missingRequired[0]);
                  }}
                  title={`Click to jump to missing required field: ${missingRequired.join(", ")}`}
                >
                  <RequiredAlertIcon size={13} />
                  <span>
                    {t("bench.reqEmptyHint", { count: missingRequired.length })}
                  </span>
                </button>
              ) : null}
              <button
                key={`exec-${shakeTick}`}
                type="button"
                className={`btn primary ${missingRequired.length > 0 && !running ? "has-missing-req" : ""} ${shakeTick > 0 && missingRequired.length > 0 ? "shake-once" : ""}`}
                disabled={running}
                onClick={() => void run(false)}
                title={
                  missingRequired.length > 0
                    ? `Missing required parameter(s): ${missingRequired.join(", ")} — click to focus first empty field`
                    : `Execute ${tool.name}`
                }
              >
                {running ? (
                  <>
                    <Spinner size={13} tone="dark" />
                    <span>{t("bench.executingMs", { ms: elapsed })}</span>
                  </>
                ) : missingRequired.length > 0 ? (
                  <>
                    <span>{t("bench.executeBtn")}</span>
                    <span className="exec-missing-badge">
                      <RequiredAlertIcon size={12} />
                      <span>{missingRequired.length}</span>
                    </span>
                  </>
                ) : (
                  t("bench.executeBtn")
                )}
              </button>
            </div>
          </div>

          {server && !isToolAllowedByPolicy(tool, policy) ? (
            <div className="callout warning callout-anim" style={{ margin: "10px 16px 0" }}>
              <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", gap: 8 }}>
                <span>
                  🛡️ <strong>Blocked by Tool Policy ({policy.mode})</strong>: Local CLI agents on{" "}
                  <code>/mcp/{slug}</code> cannot see or invoke <code>{tool.name}</code>. You can still test-run it here as Admin.
                </span>
                <button
                  type="button"
                  className="btn compact"
                  disabled={savingPolicy}
                  onClick={() => toggleSingleTool(tool)}
                >
                  Allow Tool on /mcp/{slug}
                </button>
              </div>
            </div>
          ) : null}

          <div className="workbench-split">
            {/* Request Arguments */}
            <div className="workbench-pane">
              <div className="pane-toolbar">
                <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                  <div className="segmented-tabs">
                    <button
                      type="button"
                      className={!rawMode && simple ? "active" : ""}
                      disabled={!simple}
                      onClick={() => handleModeSwitch(false)}
                    >
                      {t("bench.schemaFormTab", { count: propCount })}
                    </button>
                    <button
                      type="button"
                      className={rawMode || !simple ? "active" : ""}
                      onClick={() => handleModeSwitch(true)}
                    >
                      {t("bench.rawJsonTab")}
                    </button>
                  </div>
                  {totalRequiredCount > 0 ? (
                    missingRequired.length > 0 ? (
                      <button
                        type="button"
                        className="req-status-chip missing"
                        onClick={() => {
                          setShowMissingGuard(true);
                          setShakeTick((prev) => prev + 1);
                          focusFieldByKey(missingRequired[0]);
                        }}
                        title={`Click to focus first missing required field (${missingRequired.join(", ")})`}
                      >
                        <RequiredAlertIcon size={12} />
                        <span>
                          {t("bench.reqStatusMissing", {
                            missing: missingRequired.length,
                            total: totalRequiredCount,
                          })}
                        </span>
                      </button>
                    ) : (
                      <span className="req-status-chip ready">
                        <RequiredCheckIcon size={12} />
                        <span>
                          {t("bench.reqStatusReady", { total: totalRequiredCount })}
                        </span>
                      </span>
                    )
                  ) : null}
                </div>
                <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                  {propCount > 0 ? (
                    <button
                      type="button"
                      className="btn-link"
                      onClick={fillSample}
                      title="Populate realistic sample values from the JSON Schema"
                    >
                      {t("bench.fillSample")}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="btn-link"
                    onClick={saveCurrentPreset}
                    title="Save current arguments as a reusable preset"
                  >
                    {t("bench.savePreset")}
                  </button>
                  <CopyButton
                    text={curlSnippet}
                    label={t("bench.copyCurl")}
                    copiedLabel="✓ cURL"
                    className="btn-link"
                    onCopied={() => onNotify?.("Copied JSON-RPC cURL command")}
                  />
                  <CopyButton
                    text={raw}
                    label={t("bench.copyArgs")}
                    copiedLabel={t("common.copied")}
                    className="btn-link"
                  />
                  <button
                    type="button"
                    className="btn-link"
                    onClick={resetDefaults}
                  >
                    {t("bench.reset")}
                  </button>
                </div>
              </div>
              {presets.length > 0 ? (
                <div
                  className="row"
                  style={{
                    gap: 6,
                    padding: "6px 12px",
                    borderBottom: "1px solid var(--line)",
                    background: "rgba(255,255,255,0.015)",
                    flexWrap: "wrap",
                  }}
                >
                  <small className="muted" style={{ fontSize: 11 }}>
                    {t("bench.savedPresets")}
                  </small>
                  {presets.map((p, idx) => (
                    <span
                      key={`${p.label}-${idx}`}
                      className="chip-btn"
                      style={{ display: "inline-flex", alignItems: "center", gap: 4, padding: "2px 8px" }}
                    >
                      <button
                        type="button"
                        style={{
                          all: "unset",
                          cursor: "pointer",
                          fontSize: 11,
                          fontWeight: 600,
                        }}
                        onClick={() => applyPreset(p.args, p.label)}
                      >
                        {p.label}
                      </button>
                      <button
                        type="button"
                        style={{
                          all: "unset",
                          cursor: "pointer",
                          opacity: 0.65,
                          fontSize: 11,
                          paddingLeft: 3,
                        }}
                        title="Delete preset"
                        onClick={() => deletePreset(idx)}
                      >
                        ×
                      </button>
                    </span>
                  ))}
                </div>
              ) : null}

              <div className="pane-body">
                {showMissingGuard && missingRequired.length > 0 ? (
                  <div
                    key={`guard-${shakeTick}`}
                    className="missing-req-guard callout-anim"
                  >
                    <div className="missing-req-guard-top">
                      <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                        <span className="missing-req-icon">
                          <RequiredAlertIcon size={14} />
                        </span>
                        <strong>
                          {t("bench.missingGuardTitle", {
                            count: missingRequired.length,
                          })}
                        </strong>
                        <span className="muted" style={{ fontSize: 12 }}>
                          {t("bench.missingGuardSub")}
                        </span>
                      </div>
                      <div className="row" style={{ gap: 6, flexWrap: "wrap" }}>
                        <button
                          type="button"
                          className="btn compact"
                          onClick={fillMissingRequiredSample}
                        >
                          {t("bench.autoFillMissing")}
                        </button>
                        <button
                          type="button"
                          className="btn compact ghost"
                          onClick={() => void run(true)}
                          title="Send the tool call to the upstream server without filling required fields"
                        >
                          {t("bench.forceExecute")}
                        </button>
                      </div>
                    </div>
                    <div className="missing-req-chips">
                      {missingRequired.map((reqKey) => (
                        <button
                          key={reqKey}
                          type="button"
                          className="missing-req-chip"
                          onClick={() => focusFieldByKey(reqKey)}
                        >
                          <RequiredAlertIcon size={12} />
                          <code>{reqKey}</code>
                        </button>
                      ))}
                    </div>
                  </div>
                ) : null}

                {rawMode || !simple ? (
                  <textarea
                    className="code-editor mode-switch-anim"
                    rows={compact ? 8 : 12}
                    value={raw}
                    spellCheck={false}
                    onChange={(event) => setRaw(event.target.value)}
                  />
                ) : propCount === 0 ? (
                  <div className="no-args-notice mode-switch-anim">
                    {t("bench.noParamsNotice")}
                  </div>
                ) : (
                  <div className="mode-switch-anim">
                    <SchemaFields
                      slug={slug}
                      schema={schema}
                      values={values}
                      missingKeys={missingRequired}
                      highlightMissing={showMissingGuard}
                      shakeTick={shakeTick}
                      onNotify={onNotify}
                      onChange={updateFormValues}
                    />
                  </div>
                )}
                {error ? (
                  <div className="callout bad callout-reveal">{error}</div>
                ) : null}
              </div>
            </div>

            {/* Response Output with Pretty JSON / YAML / Raw Syntax Highlighting */}
            <div className="workbench-pane">
              {!responseObj && !rawResult && !running ? (
                <>
                  <div className="pane-toolbar">
                    <span className="muted" style={{ fontSize: 12, fontWeight: 600 }}>
                      {t("bench.responseInspector")}
                    </span>
                  </div>
                  <div className="pane-body">
                    <div className="response-placeholder">
                      <span>{t("bench.readyToExecute")}</span>
                      <small>
                        {t("bench.readyToExecuteSub", { tool: tool.name })}
                      </small>
                    </div>
                  </div>
                </>
              ) : (
                <CodeViewer
                  data={responseObj}
                  rawText={rawResult}
                  defaultMode="json"
                  loading={running}
                  loadingLabel={`Calling ${tool.name} on ${slug}…`}
                  durationMs={durationMs}
                  maxHeight={compact ? 320 : 460}
                  onNotify={onNotify}
                  statusPill={
                    responseObj ? (
                      <span
                        className={`status-pill ${responseObj.isError ? "error" : "connected"}`}
                      >
                        {responseObj.isError ? "Tool Error" : "200 OK"}
                      </span>
                    ) : null
                  }
                />
              )}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function SchemaFields({
  slug,
  schema,
  values,
  missingKeys,
  highlightMissing,
  shakeTick,
  prefix = "",
  onNotify,
  onChange,
}: {
  slug: string;
  schema?: JsonSchema;
  values: Record<string, unknown>;
  missingKeys?: string[];
  highlightMissing?: boolean;
  shakeTick?: number;
  prefix?: string;
  onNotify?: (msg: string) => void;
  onChange: (next: Record<string, unknown>) => void;
}) {
  const properties = schema?.properties ?? {};
  const required = new Set(schema?.required ?? []);
  const missingSet = new Set(missingKeys ?? []);
  return (
    <div className="schema-fields">
      {Object.entries(properties).map(([key, property]) => {
        const fullKey = prefix ? `${prefix}.${key}` : key;
        const isReq = required.has(key);
        const isMissing =
          isReq &&
          (missingSet.has(fullKey) ||
            isFieldValueMissing(values[key], property));
        const hasMissingChild = Array.from(missingSet).some((k) =>
          k.startsWith(`${fullKey}.`),
        );
        const typeLabel = property.enum
          ? "enum"
          : Array.isArray(property.type)
            ? property.type.join(" | ")
            : property.type ?? "string";
        return (
          <div
            id={`bench-card-${slug}-${fullKey}`}
            className={`schema-field-card ${isMissing || hasMissingChild ? "is-missing-req" : ""} ${highlightMissing && (isMissing || hasMissingChild) ? "is-guard-alert" : ""}`}
            key={`${key}-${highlightMissing && (isMissing || hasMissingChild) ? shakeTick : 0}`}
          >
            <div className="schema-field-head">
              <div className="row" style={{ gap: 6, flexWrap: "wrap" }}>
                <strong className="param-name">{key}</strong>
                <span className="param-type">{typeLabel}</span>
                {isReq ? (
                  <span
                    className={`param-req ${isMissing ? "is-missing" : "is-filled"}`}
                  >
                    {isMissing ? (
                      <>
                        <RequiredAlertIcon size={11} />
                        <span>required</span>
                      </>
                    ) : (
                      <>
                        <RequiredCheckIcon size={11} />
                        <span>required</span>
                      </>
                    )}
                  </span>
                ) : null}
              </div>
              {property.default !== undefined ? (
                <small className="muted">
                  default: {JSON.stringify(property.default)}
                </small>
              ) : null}
            </div>
            {property.description ? (
              <RichDescription
                text={property.description}
                variant="param"
                fieldKey={key}
                fieldSchema={property}
                onApplyFieldValue={(val, label) => {
                  const copy = { ...values, [key]: val };
                  onChange(copy);
                  onNotify?.(
                    `Applied example${label ? ` "${label}"` : ""} to "${key}"`,
                  );
                }}
                onNotify={onNotify}
              />
            ) : null}
            <FieldInput
              slug={slug}
              fieldPrefix={fullKey}
              missingKeys={missingKeys}
              highlightMissing={highlightMissing}
              shakeTick={shakeTick}
              onNotify={onNotify}
              schema={property}
              value={values[key]}
              onChange={(value) => {
                const copy = { ...values };
                if (value === undefined) {
                  delete copy[key];
                } else {
                  copy[key] = value;
                }
                onChange(copy);
              }}
            />
          </div>
        );
      })}
    </div>
  );
}

function FieldInput({
  slug,
  fieldPrefix,
  missingKeys,
  highlightMissing,
  shakeTick,
  onNotify,
  schema,
  value,
  onChange,
}: {
  slug: string;
  fieldPrefix: string;
  missingKeys?: string[];
  highlightMissing?: boolean;
  shakeTick?: number;
  onNotify?: (msg: string) => void;
  schema: JsonSchema;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  if (schema.enum?.length) {
    return (
      <select
        value={value === undefined ? "" : String(value)}
        onChange={(event) =>
          onChange(event.target.value === "" ? undefined : event.target.value)
        }
      >
        <option value="">Select an option…</option>
        {schema.enum.map((item) => (
          <option key={String(item)} value={String(item)}>
            {String(item)}
          </option>
        ))}
      </select>
    );
  }
  if (schema.type === "boolean") {
    return (
      <label className="switch-label compact">
        <input
          type="checkbox"
          checked={Boolean(value)}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span className="switch-track" />
        <span>{value ? "true" : "false"}</span>
      </label>
    );
  }
  if (schema.type === "integer" || schema.type === "number") {
    return (
      <input
        type="number"
        placeholder={schema.type === "integer" ? "0" : "0.0"}
        value={value === undefined || value === null ? "" : String(value)}
        step={schema.type === "integer" ? 1 : "any"}
        onChange={(event) =>
          onChange(
            event.target.value === "" ? undefined : Number(event.target.value),
          )
        }
      />
    );
  }
  if (schema.type === "string" || !schema.type) {
    const long = (schema.description ?? "").length > 80;
    if (long) {
      return (
        <textarea
          rows={3}
          placeholder="Enter string value…"
          value={typeof value === "string" ? value : ""}
          onChange={(event) => onChange(event.target.value)}
        />
      );
    }
    return (
      <input
        placeholder="Enter string value…"
        value={typeof value === "string" ? value : ""}
        onChange={(event) => onChange(event.target.value)}
      />
    );
  }
  if (schema.type === "array") {
    const itemType = schema.items?.type;
    const arr = Array.isArray(value) ? value : [];
    const isPrimitiveItem =
      !itemType ||
      itemType === "string" ||
      itemType === "number" ||
      itemType === "integer" ||
      itemType === "boolean";
    if (isPrimitiveItem) {
      return (
        <div className="stack" style={{ gap: 6 }}>
          {arr.map((item, idx) => (
            <div key={idx} className="row" style={{ gap: 6 }}>
              <input
                placeholder={`Item #${idx + 1} (${typeof itemType === "string" ? itemType : "string"})`}
                value={item === undefined || item === null ? "" : String(item)}
                onChange={(e) => {
                  const next = [...arr];
                  const rawVal = e.target.value;
                  next[idx] =
                    itemType === "number" || itemType === "integer"
                      ? rawVal === ""
                        ? 0
                        : Number(rawVal)
                      : itemType === "boolean"
                        ? rawVal === "true"
                        : rawVal;
                  onChange(next);
                }}
              />
              <button
                type="button"
                className="btn compact ghost"
                onClick={() => {
                  const next = arr.filter((_, i) => i !== idx);
                  onChange(next.length ? next : undefined);
                }}
                title="Remove item"
              >
                ×
              </button>
            </div>
          ))}
          <div>
            <button
              type="button"
              className="btn compact"
              onClick={() => {
                const defaultItem =
                  itemType === "number" || itemType === "integer"
                    ? 0
                    : itemType === "boolean"
                      ? false
                      : "";
                onChange([...arr, defaultItem]);
              }}
            >
              + Add Item
            </button>
          </div>
        </div>
      );
    }
  }
  if (schema.type === "object" && schema.properties) {
    const nestedObj =
      value && typeof value === "object" && !Array.isArray(value)
        ? (value as Record<string, unknown>)
        : {};
    return (
      <div
        style={{
          padding: 10,
          borderRadius: 8,
          border: "1px solid var(--line)",
          background: "rgba(255,255,255,0.02)",
        }}
      >
        <SchemaFields
          slug={slug}
          prefix={fieldPrefix}
          missingKeys={missingKeys}
          highlightMissing={highlightMissing}
          shakeTick={shakeTick}
          onNotify={onNotify}
          schema={schema}
          values={nestedObj}
          onChange={(nextNested) =>
            onChange(Object.keys(nextNested).length ? nextNested : undefined)
          }
        />
      </div>
    );
  }
  return (
    <textarea
      rows={4}
      placeholder={schema.type === "array" ? '[\n  "item"\n]' : '{\n  "key": "value"\n}'}
      value={
        value === undefined
          ? ""
          : typeof value === "string"
            ? value
            : JSON.stringify(value, null, 2)
      }
      onChange={(event) => {
        try {
          onChange(
            event.target.value ? JSON.parse(event.target.value) : undefined,
          );
        } catch {
          onChange(event.target.value);
        }
      }}
    />
  );
}

interface ExtractedExample {
  title?: string;
  rawCode: string;
  prettyCode: string;
  isJson: boolean;
  parsedJson?: unknown;
}

function tryParseLooseJson(raw: string): { ok: boolean; value?: unknown } {
  const trimmed = raw.trim();
  if (!trimmed) return { ok: false };
  try {
    return { ok: true, value: JSON.parse(trimmed) };
  } catch {
    // Attempt to repair unescaped quotes inside string values, e.g.:
    // {"view_id": "abc123", "configure": "FILTER "Status" = "Done""}
    if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
      try {
        const repaired = trimmed.replace(
          /:\s*"((?:[^"\\]|\\.)*)"\s*(?=[,}])/g,
          (_match, inner: string) => {
            const escapedInner = inner.replace(/\\"/g, '"').replace(/"/g, '\\"');
            return `: "${escapedInner}"`;
          },
        );
        return { ok: true, value: JSON.parse(repaired) };
      } catch {
        return { ok: false };
      }
    }
    return { ok: false };
  }
}

function buildExtractedExample(rawCode: string, title?: string): ExtractedExample {
  const clean = rawCode.trim();
  const parsed = tryParseLooseJson(clean);
  if (parsed.ok && parsed.value !== undefined && typeof parsed.value === "object" && parsed.value !== null) {
    return {
      title: title?.trim() || undefined,
      rawCode: clean,
      prettyCode: JSON.stringify(parsed.value, null, 2),
      isJson: true,
      parsedJson: parsed.value,
    };
  }
  return {
    title: title?.trim() || undefined,
    rawCode: clean,
    prettyCode: clean,
    isJson: false,
  };
}

function stripDescriptionForPreview(raw: string): string {
  return raw
    .replace(/<examples\b[^>]*>[\s\S]*?<\/examples>/gi, "")
    .replace(/<example\b[^>]*>[\s\S]*?<\/example>/gi, "")
    .replace(/```[\s\S]*?```/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function parseRichDescription(raw: string): {
  proseBlocks: string[];
  examples: ExtractedExample[];
} {
  const examples: ExtractedExample[] = [];

  // 1. Extract <examples>...</examples> blocks
  let working = raw.replace(
    /<examples\b[^>]*>([\s\S]*?)<\/examples>/gi,
    (_full, inner: string) => {
      const lines = inner
        .split(/\n+/)
        .map((l) => l.trim())
        .filter(Boolean);
      let matchedNumbered = false;
      for (const line of lines) {
        const m = /^(?:\d+\.\s*|[-*]\s*)([^:{]+):\s*(\{[\s\S]*\})\s*$/.exec(line);
        if (m) {
          matchedNumbered = true;
          examples.push(buildExtractedExample(m[2], m[1].trim()));
        }
      }
      if (!matchedNumbered && inner.trim()) {
        examples.push(buildExtractedExample(inner.trim(), "Examples"));
      }
      return "\n\n";
    },
  );

  // 2. Extract <example description="...">...</example> and <example>...</example>
  working = working.replace(
    /<example(?:\s+description="([^"]*)")?\s*>([\s\S]*?)<\/example>\.?/gi,
    (_full, descAttr: string | undefined, inner: string) => {
      if (inner.trim()) {
        examples.push(buildExtractedExample(inner, descAttr));
      }
      return "\n\n";
    },
  );

  // 3. Extract Markdown fenced code blocks ```json ... ```
  working = working.replace(
    /```([a-zA-Z0-9_-]*)\n([\s\S]*?)```/g,
    (_full, lang: string, inner: string) => {
      if (inner.trim()) {
        examples.push(
          buildExtractedExample(
            inner,
            lang ? `${lang.toUpperCase()} Snippet` : "Code Snippet",
          ),
        );
      }
      return "\n\n";
    },
  );

  // Clean trailing "Example:" or "Examples:" left over right before an extracted <example>
  working = working.replace(/(?:^|\s)Examples?:\s*$/gim, "");

  const proseBlocks = working
    .split(/\n{2,}/)
    .map((b) => b.trim())
    .filter(Boolean);

  return { proseBlocks, examples };
}

function renderInlineTokens(text: string): ReactNode {
  const tokenRegex =
    /(`[^`]+`|<[a-zA-Z0-9_-]+(?:\s+[^<>]*)?>|\b[a-z_]+(?:\.[a-z_]+)+(?:=[a-z0-9_"']+)?|\b[a-z_]+="[^"]+")/g;
  const parts: ReactNode[] = [];
  let lastIdx = 0;
  let match: RegExpExecArray | null;
  let k = 0;

  while ((match = tokenRegex.exec(text)) !== null) {
    if (match.index > lastIdx) {
      parts.push(text.slice(lastIdx, match.index));
    }
    const rawToken = match[0];
    const cleanToken =
      rawToken.startsWith("`") && rawToken.endsWith("`")
        ? rawToken.slice(1, -1)
        : rawToken;
    parts.push(
      <code key={k++} className="desc-inline-code">
        {cleanToken}
      </code>,
    );
    lastIdx = match.index + rawToken.length;
  }

  if (lastIdx < text.length) {
    parts.push(text.slice(lastIdx));
  }
  return parts;
}

function RichDescription({
  text,
  variant = "tool",
  fieldKey,
  fieldSchema,
  onApplyArgs,
  onApplyFieldValue,
  onNotify,
}: {
  text: string;
  variant?: "tool" | "param";
  fieldKey?: string;
  fieldSchema?: JsonSchema;
  onApplyArgs?: (args: Record<string, unknown>, label?: string) => void;
  onApplyFieldValue?: (val: unknown, label?: string) => void;
  onNotify?: (msg: string) => void;
}) {
  const { t } = useI18n();
  const { proseBlocks, examples } = useMemo(
    () => parseRichDescription(text),
    [text],
  );
  const [expandedProse, setExpandedProse] = useState(false);

  const isLongProse =
    variant === "tool"
      ? proseBlocks.length > 2 || text.length > 520
      : proseBlocks.length > 2 || text.length > 420;
  const visibleBlocks =
    isLongProse && !expandedProse ? proseBlocks.slice(0, 1) : proseBlocks;

  function resolveFieldValue(ex: ExtractedExample): unknown {
    if (ex.isJson && ex.parsedJson !== undefined) {
      if (
        fieldKey &&
        ex.parsedJson &&
        typeof ex.parsedJson === "object" &&
        !Array.isArray(ex.parsedJson)
      ) {
        const obj = ex.parsedJson as Record<string, unknown>;
        const keys = Object.keys(obj);
        if (
          keys.length === 1 &&
          keys[0] === fieldKey &&
          !(fieldSchema?.properties && fieldKey in fieldSchema.properties)
        ) {
          return obj[fieldKey];
        }
      }
      if (fieldSchema?.type === "string") {
        return JSON.stringify(ex.parsedJson, null, 2);
      }
      return ex.parsedJson;
    }
    if (fieldSchema?.type === "number" || fieldSchema?.type === "integer") {
      const n = Number(ex.rawCode);
      return Number.isNaN(n) ? ex.rawCode : n;
    }
    if (fieldSchema?.type === "boolean") {
      return ex.rawCode === "true";
    }
    return ex.rawCode;
  }

  return (
    <div className={`rich-desc ${variant}`}>
      {visibleBlocks.length > 0 ? (
        <div className="rich-desc-prose">
          {visibleBlocks.map((block, idx) => {
            const lines = block.split("\n");
            const isBulletList =
              lines.length > 1 &&
              lines.every(
                (l) =>
                  l.trim().startsWith("- ") ||
                  l.trim().startsWith("* ") ||
                  !l.trim(),
              );
            if (isBulletList) {
              return (
                <ul key={idx} className="rich-desc-list">
                  {lines
                    .map((l) => l.trim())
                    .filter(Boolean)
                    .map((line, lIdx) => (
                      <li key={lIdx}>
                        {renderInlineTokens(line.replace(/^[-*]\s+/, ""))}
                      </li>
                    ))}
                </ul>
              );
            }
            return (
              <p key={idx} className={variant === "tool" ? "tool-desc" : "param-desc"}>
                {renderInlineTokens(block)}
              </p>
            );
          })}
          {isLongProse ? (
            <button
              type="button"
              className="desc-expand-btn"
              onClick={() => setExpandedProse((prev) => !prev)}
            >
              {expandedProse
                ? t("bench.collapseSpec")
                : t("bench.expandSpec", { count: proseBlocks.length })}
            </button>
          ) : null}
        </div>
      ) : null}

      {examples.length > 0 ? (
        <div className={`desc-examples-wrap ${variant}`}>
          <div className="desc-examples-header">
            <span className="desc-examples-title">
              ⟨/⟩{" "}
              {examples.length === 1
                ? t("bench.examplesSingle")
                : t("bench.examplesPlural", { count: examples.length })}
            </span>
            <span className="muted" style={{ fontSize: 11 }}>
              {variant === "tool"
                ? t("bench.examplesHintTool")
                : t("bench.examplesHintParam")}
            </span>
          </div>
          <div className={`desc-examples-grid ${variant}`}>
            {examples.map((ex, idx) => {
              const canApplyToolArgs =
                variant === "tool" &&
                Boolean(onApplyArgs) &&
                ex.isJson &&
                ex.parsedJson !== null &&
                typeof ex.parsedJson === "object" &&
                !Array.isArray(ex.parsedJson);
              const canApplyParam =
                variant === "param" && Boolean(onApplyFieldValue);

              return (
                <div key={idx} className="desc-example-card">
                  <div className="desc-example-bar">
                    <div className="row" style={{ gap: 6, minWidth: 0 }}>
                      <span className="desc-example-badge">
                        EXAMPLE {examples.length > 1 ? `#${idx + 1}` : ""}
                      </span>
                      {ex.title ? (
                        <strong className="desc-example-label">
                          {ex.title}
                        </strong>
                      ) : null}
                    </div>
                    <div className="row" style={{ gap: 6, flexShrink: 0 }}>
                      {canApplyToolArgs ? (
                        <button
                          type="button"
                          className="desc-example-apply-btn"
                          onClick={() =>
                            onApplyArgs?.(
                              ex.parsedJson as Record<string, unknown>,
                              ex.title || `#${idx + 1}`,
                            )
                          }
                          title="Populate Schema Form and Raw JSON with this example"
                        >
                          {t("bench.applyToForm")}
                        </button>
                      ) : null}
                      {canApplyParam ? (
                        <button
                          type="button"
                          className="desc-example-apply-btn"
                          onClick={() =>
                            onApplyFieldValue?.(
                              resolveFieldValue(ex),
                              ex.title || `#${idx + 1}`,
                            )
                          }
                          title="Fill this parameter with the example value"
                        >
                          {t("bench.useExample")}
                        </button>
                      ) : null}
                      <CopyButton
                        text={ex.prettyCode}
                        label={t("common.copy")}
                        copiedLabel="✓"
                        className="desc-example-copy-btn"
                        onCopied={() =>
                          onNotify?.(
                            `Copied example${ex.title ? ` "${ex.title}"` : ""}`,
                          )
                        }
                      />
                    </div>
                  </div>
                  <pre className="desc-example-pre">
                    <code>
                      {ex.isJson ? highlightJson(ex.prettyCode) : ex.prettyCode}
                    </code>
                  </pre>
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function isFieldValueMissing(value: unknown, propSchema?: JsonSchema): boolean {
  if (value === undefined || value === null) return true;
  const t = propSchema?.type;
  if (t === "boolean") {
    return typeof value !== "boolean";
  }
  if (t === "number" || t === "integer") {
    return typeof value !== "number" || Number.isNaN(value);
  }
  if (t === "array") {
    if (!Array.isArray(value) || value.length === 0) return true;
    return value.every(
      (item) =>
        item === undefined ||
        item === null ||
        (typeof item === "string" && item.trim() === ""),
    );
  }
  if (t === "object") {
    if (typeof value !== "object" || Array.isArray(value)) return true;
    const obj = value as Record<string, unknown>;
    if (propSchema?.required?.length) {
      return getMissingRequiredFields(propSchema, obj).length > 0;
    }
    return Object.keys(obj).length === 0;
  }
  if (typeof value === "string") {
    return value.trim() === "";
  }
  return false;
}

export function getMissingRequiredFields(
  schema: JsonSchema | undefined,
  args: Record<string, unknown>,
  prefix = "",
): string[] {
  if (!schema) return [];
  const props = schema.properties ?? {};
  const required = schema.required ?? [];
  const missing: string[] = [];

  for (const key of required) {
    const fullKey = prefix ? `${prefix}.${key}` : key;
    const propSchema = props[key];
    const val = args?.[key];
    if (
      propSchema?.type === "object" &&
      propSchema.properties &&
      val &&
      typeof val === "object" &&
      !Array.isArray(val)
    ) {
      const nestedMissing = getMissingRequiredFields(
        propSchema,
        val as Record<string, unknown>,
        fullKey,
      );
      missing.push(...nestedMissing);
    } else if (isFieldValueMissing(val, propSchema)) {
      missing.push(fullKey);
    }
  }

  for (const [key, propSchema] of Object.entries(props)) {
    if (required.includes(key)) continue;
    const fullKey = prefix ? `${prefix}.${key}` : key;
    const val = args?.[key];
    if (
      propSchema?.type === "object" &&
      propSchema.properties &&
      val &&
      typeof val === "object" &&
      !Array.isArray(val)
    ) {
      missing.push(
        ...getMissingRequiredFields(
          propSchema,
          val as Record<string, unknown>,
          fullKey,
        ),
      );
    }
  }

  return missing;
}

function isSimpleObject(schema?: JsonSchema) {
  if (!schema) return true;
  if (schema.type === "object" || schema.properties) return true;
  return false;
}

function defaultsFrom(schema?: JsonSchema): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  const properties = schema?.properties ?? {};
  for (const [key, property] of Object.entries(properties)) {
    if (property.default !== undefined) out[key] = property.default;
    else if (property.type === "boolean") out[key] = false;
  }
  return out;
}

function generateSampleFromSchema(schema?: JsonSchema): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  const properties = schema?.properties ?? {};
  for (const [key, property] of Object.entries(properties)) {
    if (property.default !== undefined) {
      out[key] = property.default;
    } else if (property.enum && property.enum.length > 0) {
      out[key] = property.enum[0];
    } else if (property.type === "boolean") {
      out[key] = true;
    } else if (property.type === "integer" || property.type === "number") {
      out[key] = key.toLowerCase().includes("limit") ? 10 : 1;
    } else if (property.type === "array") {
      const itemType = property.items?.type;
      out[key] =
        itemType === "number" || itemType === "integer" ? [1] : ["sample"];
    } else if (property.type === "object" && property.properties) {
      out[key] = generateSampleFromSchema(property);
    } else {
      const lower = key.toLowerCase();
      if (lower.includes("query") || lower.includes("search")) {
        out[key] = "roadmap";
      } else if (lower.includes("url") || lower.includes("uri")) {
        out[key] = "https://example.com";
      } else if (lower.includes("id")) {
        out[key] = "sample-id-1";
      } else {
        out[key] = `sample_${key}`;
      }
    }
  }
  return out;
}
