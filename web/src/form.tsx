import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { api } from "./api";
import { ToolBench } from "./bench";
import { useI18n } from "./i18n";
import { ServiceIcon } from "./icons";
import type {
  ActivityEntry,
  AuthKind,
  Discovery,
  HeaderPair,
  ImportedCandidate,
  ImportResponse,
  OAuthGrant,
  ProbeResult,
  RegistryEntry,
  ServerInput,
  ServerView,
  Transport,
} from "./types";
import { LiveTerminalGadget, Spinner, useElapsedTimer } from "./ui";

const STEPS = [
  { id: 0, num: "01", title: "Identity & Routing", subtitle: "Name, slug & upstream URL" },
  { id: 1, num: "02", title: "Credential Vault", subtitle: "Isolated authentication" },
  { id: 2, num: "03", title: "Verify & Save", subtitle: "Live probe & tool execution" },
];

const AUTH_OPTIONS: {
  kind: AuthKind;
  title: string;
  badge: string;
  desc: string;
}[] = [
  {
    kind: "none",
    title: "No Authentication",
    badge: "Open",
    desc: "Connect directly without injecting credential headers.",
  },
  {
    kind: "bearer",
    title: "Bearer Token",
    badge: "API Key / JWT",
    desc: "Injects Authorization: Bearer <token> on every upstream call.",
  },
  {
    kind: "oauth2",
    title: "OAuth 2.0",
    badge: "Auto-Refresh",
    desc: "PKCE browser flow or Client Credentials with automatic token renewal.",
  },
  {
    kind: "basic",
    title: "HTTP Basic Auth",
    badge: "User + Pass",
    desc: "Base64-encoded username & password with optional realm verification.",
  },
  {
    kind: "headers",
    title: "Custom Headers",
    badge: "Key / Value",
    desc: "Attach custom secret headers such as X-API-Key or X-Workspace-Token.",
  },
];

export function ServerForm({
  initial,
  publicBaseUrl = "http://127.0.0.1:8080",
  activity = [],
  onClearActivity,
  onSaved,
  onNotify,
}: {
  initial?: ServerView;
  publicBaseUrl?: string;
  activity?: ActivityEntry[];
  onClearActivity?: () => void;
  onSaved?: (msg: string) => void;
  onNotify?: (msg: string) => void;
}) {
  const navigate = useNavigate();
  const { t } = useI18n();
  const [searchParams, setSearchParams] = useSearchParams();
  const autoImportedRef = useRef(false);
  const [step, setStep] = useState(0);
  const [stepDir, setStepDir] = useState<"forward" | "backward">("forward");
  const [name, setName] = useState(initial?.name ?? "");
  const [slug, setSlug] = useState(initial?.slug ?? "");
  const [slugTouched, setSlugTouched] = useState(Boolean(initial));
  const [url, setUrl] = useState(initial?.url ?? "");
  const [transport, setTransport] = useState<Transport>(initial?.transport ?? "streamable-http");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [kind, setKind] = useState<AuthKind>(initial?.auth.kind ?? "none");

  const [bearer, setBearer] = useState("");
  const [showBearer, setShowBearer] = useState(false);

  const [username, setUsername] = useState(initial?.auth.basicUsername ?? "");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [realm, setRealm] = useState(initial?.auth.basicRealm ?? "");

  const [headers, setHeaders] = useState<HeaderPair[]>(
    initial?.auth.headerNames.length
      ? initial.auth.headerNames.map((n) => ({ name: n, value: "" }))
      : [{ name: "", value: "" }],
  );
  const [visibleHeaders, setVisibleHeaders] = useState<Record<number, boolean>>({});

  const [grant, setGrant] = useState<OAuthGrant>(initial?.auth.oauth?.grant ?? "authorization_code");
  const [clientId, setClientId] = useState(initial?.auth.oauth?.clientId ?? "");
  const [clientSecret, setClientSecret] = useState("");
  const [showClientSecret, setShowClientSecret] = useState(false);
  const [authorizeUrl, setAuthorizeUrl] = useState(initial?.auth.oauth?.authorizeUrl ?? "");
  const [tokenUrl, setTokenUrl] = useState(initial?.auth.oauth?.tokenUrl ?? "");
  const [scopes, setScopes] = useState(initial?.auth.oauth?.scopes.join(" ") ?? "");
  const [accessToken, setAccessToken] = useState("");
  const [refreshToken, setRefreshToken] = useState("");
  const [showManualTokens, setShowManualTokens] = useState(false);

  const [discovery, setDiscovery] = useState<Discovery | null>(null);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [selectedProbeTool, setSelectedProbeTool] = useState<string>("");
  const [showProbeSandbox, setShowProbeSandbox] = useState<boolean>(true);
  const [importCommand, setImportCommand] = useState<string>("");
  const [importResult, setImportResult] = useState<ImportResponse | null>(null);
  const [selectedImportIdx, setSelectedImportIdx] = useState<number>(0);
  const [registry, setRegistry] = useState<RegistryEntry[]>([]);
  const [showRegistryDrawer, setShowRegistryDrawer] = useState<boolean>(false);
  const [registryQuery, setRegistryQuery] = useState<string>("");
  const [error, setError] = useState("");
  const [busyAction, setBusyAction] = useState<"import" | "discover" | "probe" | "save" | null>(null);
  const [busyLabel, setBusyLabel] = useState("");
  const busy = busyAction !== null;
  const elapsedMs = useElapsedTimer(busy);

  const originChanged = useMemo(() => {
    if (!initial) return false;
    return !isSameOrigin(initial.url, url);
  }, [initial, url]);

  const slugValid = useMemo(() => isValidSlug(slug), [slug]);
  const urlValid = useMemo(() => isValidHttpUrl(url), [url]);
  const identityReady = Boolean(name.trim() && slugValid && urlValid);

  const needsManualSecret = useMemo(() => {
    if (kind === "bearer") return !bearer.trim();
    if (kind === "basic") return !username.trim() || !password.trim();
    if (kind === "headers") {
      return !headers.some((h) => h.name.trim() && h.value.trim());
    }
    if (kind === "oauth2") {
      if (grant === "authorization_code") {
        return !authorizeUrl.trim() || !tokenUrl.trim();
      }
      return !tokenUrl.trim() || !clientId.trim() || !clientSecret.trim();
    }
    return false;
  }, [
    kind,
    bearer,
    username,
    password,
    headers,
    grant,
    authorizeUrl,
    tokenUrl,
    clientId,
    clientSecret,
  ]);

  const canQuickRegisterImported = Boolean(identityReady && !needsManualSecret);

  function changeStep(next: number) {
    setStepDir(next >= step ? "forward" : "backward");
    setStep(next);
  }

  function updateName(value: string) {
    setName(value);
    if (!slugTouched) setSlug(slugify(value));
  }

  function applyImportedCandidate(candidate: ImportedCandidate, idx = 0) {
    setSelectedImportIdx(idx);
    const inp = candidate.input;
    setName(inp.name);
    setSlug(inp.slug);
    setSlugTouched(true);
    setUrl(inp.url);
    setTransport(inp.transport ?? "streamable-http");
    setEnabled(inp.enabled ?? true);
    setKind(inp.auth.kind);
    setBearer(inp.auth.bearerToken ?? "");
    setUsername(inp.auth.basicUsername ?? "");
    setPassword(inp.auth.basicPassword ?? "");
    setRealm(inp.auth.basicRealm ?? "");
    setHeaders(
      inp.auth.headers?.length ? inp.auth.headers : [{ name: "", value: "" }],
    );
    if (inp.auth.oauth) {
      setGrant(inp.auth.oauth.grant ?? "authorization_code");
      setClientId(inp.auth.oauth.clientId ?? "");
      setClientSecret(inp.auth.oauth.clientSecret ?? "");
      setAuthorizeUrl(inp.auth.oauth.authorizeUrl ?? "");
      setTokenUrl(inp.auth.oauth.tokenUrl ?? "");
      setScopes((inp.auth.oauth.scopes ?? []).join(" "));
    }
    if (candidate.discovery) {
      setDiscovery(candidate.discovery);
      if (candidate.discovery.authorizationEndpoint) {
        setAuthorizeUrl(candidate.discovery.authorizationEndpoint);
      }
      if (candidate.discovery.tokenEndpoint) {
        setTokenUrl(candidate.discovery.tokenEndpoint);
      }
      if (candidate.discovery.scopes?.length) {
        setScopes(candidate.discovery.scopes.join(" "));
      }
    }
  }

  async function runImportCommand(customCmd?: string) {
    const rawCmd = (customCmd ?? importCommand).trim();
    if (!rawCmd) {
      setError("Paste a CLI install command (claude, agy, codex, gemini, cursor) or JSON/TOML snippet first.");
      return;
    }
    if (customCmd !== undefined) {
      setImportCommand(customCmd);
    }
    setBusyAction("import");
    setBusyLabel("Parsing command & discovering upstream metadata…");
    setError("");
    try {
      const res = await api<ImportResponse>("/api/import/parse", {
        method: "POST",
        body: JSON.stringify({ command: rawCmd, autoDiscover: true }),
      });
      setImportResult(res);
      if (res.candidates.length > 0) {
        applyImportedCandidate(res.candidates[0], 0);
        onNotify?.(
          `Imported ${res.candidates[0].input.name} (${res.sourceLabel})`,
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to parse install command");
    } finally {
      setBusyAction(null);
      setBusyLabel("");
    }
  }

  useEffect(() => {
    api<RegistryEntry[]>("/api/registry")
      .then(setRegistry)
      .catch(() => setRegistry([]));
  }, []);

  useEffect(() => {
    if (initial || autoImportedRef.current) return;
    const presetCmd = searchParams.get("import");
    const presetStep = searchParams.get("step");
    if (presetCmd && presetCmd.trim()) {
      autoImportedRef.current = true;
      setImportCommand(presetCmd);
      const nextParams = new URLSearchParams(searchParams);
      nextParams.delete("import");
      nextParams.delete("step");
      setSearchParams(nextParams, { replace: true });
      void runImportCommand(presetCmd).then(() => {
        if (presetStep === "1") {
          setStep(1);
        }
      });
    }
  }, [initial, searchParams, setSearchParams]);

  function payload(): ServerInput {
    return {
      name: name.trim(),
      slug: slug.trim(),
      url: url.trim(),
      transport,
      enabled,
      serverId: initial?.id,
      auth: {
        kind,
        bearerToken: blank(bearer),
        basicUsername: blank(username),
        basicPassword: blank(password),
        basicRealm: blank(realm),
        headers: headers.filter((header) => header.name.trim()),
        oauth:
          kind === "oauth2"
            ? {
                grant,
                clientId: clientId.trim(),
                clientSecret: blank(clientSecret),
                authorizeUrl: blank(authorizeUrl),
                tokenUrl: tokenUrl.trim(),
                scopes: scopes.split(/[\s,]+/).filter(Boolean),
                accessToken: blank(accessToken),
                refreshToken: blank(refreshToken),
              }
            : null,
      },
    };
  }

  async function discover() {
    if (!urlValid) {
      setError("Enter a valid HTTP/HTTPS MCP endpoint URL first.");
      return;
    }
    setBusyAction("discover");
    setBusyLabel("Discovering OAuth metadata…");
    setError("");
    try {
      const found = await api<Discovery>(`/api/discover?url=${encodeURIComponent(url.trim())}`);
      setDiscovery(found);
      if (found.authorizationEndpoint) setAuthorizeUrl(found.authorizationEndpoint);
      if (found.tokenEndpoint) setTokenUrl(found.tokenEndpoint);
      if (found.scopes?.length) setScopes(found.scopes.join(" "));
      if (found.realm) setRealm(found.realm);
      setProbe({
        ok: !found.authRequired,
        message: found.message,
        tools: [],
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : "OAuth discovery failed");
    } finally {
      setBusyAction(null);
      setBusyLabel("");
    }
  }

  async function testConnection() {
    setBusyAction("probe");
    setBusyLabel("Testing initialize & tools/list…");
    setError("");
    try {
      const result = await api<ProbeResult>("/api/probe", {
        method: "POST",
        body: JSON.stringify(payload()),
      });
      setProbe(result);
      if (result.ok && result.tools?.length) {
        setSelectedProbeTool(result.tools[0].name);
        setShowProbeSandbox(true);
      }
      onNotify?.(
        result.ok
          ? `Handshake verified (${result.tools.length} tools discovered)`
          : "Connection probe failed",
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : "Connection test failed");
    } finally {
      setBusyAction(null);
      setBusyLabel("");
    }
  }

  async function save() {
    setBusyAction("save");
    setBusyLabel(initial ? "Saving changes…" : "Registering upstream…");
    setError("");
    try {
      const body = JSON.stringify(payload());
      const saved = initial
        ? await api<ServerView>(`/api/servers/${initial.id}`, { method: "PUT", body })
        : await api<ServerView>("/api/servers", { method: "POST", body });
      if (kind === "oauth2" && grant === "authorization_code" && !saved.auth.oauth?.hasAccessToken) {
        setBusyLabel("Redirecting to OAuth provider…");
        const started = await api<{ authorizeUrl: string }>(`/api/servers/${saved.id}/oauth/start`, {
          method: "POST",
        });
        window.location.href = started.authorizeUrl;
        return;
      }
      onSaved?.(initial ? `Updated ${saved.slug}` : `Added ${saved.slug}`);
      navigate(`/servers/${saved.id}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save upstream");
    } finally {
      setBusyAction(null);
      setBusyLabel("");
    }
  }

  function updateHeader(index: number, next: HeaderPair) {
    setHeaders(headers.map((header, item) => (item === index ? next : header)));
  }

  const canKeepStoredBearer = Boolean(initial?.auth.bearerConfigured && !originChanged);
  const canKeepStoredBasic = Boolean(initial?.auth.kind === "basic" && !originChanged);
  const canKeepStoredHeaders = Boolean(initial?.auth.kind === "headers" && !originChanged);
  const canKeepStoredOAuthSecret = Boolean(initial?.auth.oauth?.hasClientSecret && !originChanged);

  const probeEntries = useMemo(() => {
    const currentSlug = slug.trim();
    return activity.filter(
      (e) => e.source === "probe" || (currentSlug && e.slug === currentSlug),
    );
  }, [activity, slug]);

  return (
    <div className="wizard-card">
      {/* Interactive Stepper */}
      <div className="wizard-stepper" role="tablist">
        {STEPS.map((item) => {
          const active = step === item.id;
          const completed = item.id < step || (item.id === 0 && identityReady);
          const localizedTitle =
            item.id === 0
              ? t("form.step1")
              : item.id === 1
                ? t("form.step2")
                : t("form.step3");
          return (
            <button
              key={item.id}
              type="button"
              role="tab"
              aria-selected={active}
              className={`step-tab ${active ? "active" : ""} ${completed ? "completed" : ""}`}
              onClick={() => changeStep(item.id)}
            >
              <span className="step-num">{item.num}</span>
              <span className="step-meta">
                <strong>{localizedTitle}</strong>
                <small>{item.subtitle}</small>
              </span>
            </button>
          );
        })}
      </div>

      {originChanged && initial && initial.auth.kind !== "none" ? (
        <div className="callout warning callout-anim">
          <div className="callout-title">Security Boundary: Endpoint Origin Changed</div>
          <p>
            You changed the upstream origin from <code>{extractOrigin(initial.url)}</code> to{" "}
            <code>{extractOrigin(url) || "a new host"}</code>. To prevent credential exfiltration, stored secrets will{" "}
            <strong>not</strong> be reused automatically—please re-enter credentials in Step 02.
          </p>
        </div>
      ) : null}

      {/* STEP 0: IDENTITY & ROUTING */}
      {step === 0 ? (
        <div key="step-0" className={`wizard-body step-anim-${stepDir}`}>
          <div className="form-section import-compat-section">
            <div className="section-header">
              <div className="import-header-row">
                <h3>CLI / Plugin Compatibility Importer</h3>
                <div className="compat-badges">
                  <span className="badge-tag">Claude</span>
                  <span className="badge-tag">AGY</span>
                  <span className="badge-tag">Codex</span>
                  <span className="badge-tag">Gemini</span>
                  <span className="badge-tag">Cursor</span>
                  <span className="badge-tag subtle">JSON / TOML</span>
                </div>
              </div>
              <p>
                Paste any external MCP installation command or config snippet (e.g.{" "}
                <code>agy plugin install …</code>, <code>claude plugin install …</code>,{" "}
                <code>claude mcp add …</code>, <code>codex mcp add …</code>, Cursor deeplinks, or{" "}
                <code>mcp_config.json</code>) to auto-resolve the upstream endpoint and OAuth settings.
              </p>
            </div>

            <div className="import-preset-row">
              <span className="muted" style={{ fontSize: "11px" }}>Try example:</span>
              {[
                {
                  slug: "notion",
                  label: "agy plugin: Notion",
                  cmd: "agy plugin install notion@antigravity-plugins-partners",
                },
                {
                  slug: "stripe",
                  label: "agy plugin: Stripe",
                  cmd: "agy plugin install stripe",
                },
                {
                  slug: "linear",
                  label: "claude plugin: Linear",
                  cmd: "claude plugin install linear",
                },
                {
                  slug: "github",
                  label: "claude mcp add: GitHub",
                  cmd: 'claude mcp add --transport http github https://api.githubcopilot.com/mcp/ --header "Authorization: Bearer ${GITHUB_PERSONAL_ACCESS_TOKEN}"',
                },
                {
                  slug: "context7",
                  label: "codex mcp add: Context7",
                  cmd: "codex mcp add context7 --url https://mcp.context7.com/mcp",
                },
              ].map((preset) => (
                <button
                  key={preset.label}
                  type="button"
                  className="chip-btn preset-with-icon"
                  disabled={busy}
                  onClick={() => runImportCommand(preset.cmd)}
                >
                  <ServiceIcon slug={preset.slug} name={preset.label} size={16} />
                  <span>{preset.label}</span>
                </button>
              ))}
              {registry.length > 0 ? (
                <button
                  type="button"
                  className={`chip-btn ${showRegistryDrawer ? "active" : ""}`}
                  disabled={busy}
                  onClick={() => setShowRegistryDrawer((v) => !v)}
                >
                  {showRegistryDrawer
                    ? `▾ Hide Service Registry (${registry.length})`
                    : `▸ Browse Service Registry (${registry.length})`}
                </button>
              ) : null}
            </div>

            {showRegistryDrawer && registry.length > 0 ? (
              <div className="registry-catalog callout-anim" style={{ marginBottom: 12 }}>
                <div className="registry-toolbar">
                  <span className="muted" style={{ fontSize: 12 }}>
                    Click any service below to auto-fill its endpoint &amp; discover OAuth settings:
                  </span>
                  <input
                    type="search"
                    className="registry-search-input"
                    value={registryQuery}
                    placeholder="Filter services (Notion, Linear, GitHub, Jira, Sentry…)"
                    onChange={(e) => setRegistryQuery(e.target.value)}
                  />
                </div>
                <div className="registry-grid compact">
                  {registry
                    .filter((item) => {
                      const q = registryQuery.trim().toLowerCase();
                      if (!q) return true;
                      return (
                        item.name.toLowerCase().includes(q) ||
                        item.slug.toLowerCase().includes(q) ||
                        item.provider.toLowerCase().includes(q) ||
                        item.description.toLowerCase().includes(q)
                      );
                    })
                    .map((item) => (
                      <div key={item.slug} className="registry-card">
                        <div className="registry-card-top">
                          <div
                            className="row"
                            style={{
                              gap: 10,
                              alignItems: "flex-start",
                              flexWrap: "nowrap",
                              minWidth: 0,
                              flex: 1,
                            }}
                          >
                            <ServiceIcon slug={item.slug} name={item.name} size={28} />
                            <div style={{ minWidth: 0, flex: 1 }}>
                              <div className="registry-card-title">
                                <strong>{item.name}</strong>
                                <span className="registry-provider">{item.provider}</span>
                              </div>
                              <code className="registry-slug">/mcp/{item.slug}</code>
                            </div>
                          </div>
                          <span
                            className={`registry-auth-badge ${
                              item.authKind === "oauth2"
                                ? "oauth"
                                : item.authKind === "none"
                                  ? "open"
                                  : "secret"
                            }`}
                          >
                            {item.authLabel}
                          </span>
                        </div>
                        <p className="registry-desc">{item.description}</p>
                        <div className="registry-card-foot">
                          <code className="registry-url">{item.url}</code>
                          <button
                            type="button"
                            className="btn primary compact"
                            disabled={busy}
                            onClick={() => {
                              setShowRegistryDrawer(false);
                              void runImportCommand(item.installCommand);
                            }}
                          >
                            Select &amp; Auto-Fill →
                          </button>
                        </div>
                      </div>
                    ))}
                </div>
              </div>
            ) : null}

            <div className="import-input-row">
              <textarea
                rows={2}
                className="import-textarea"
                value={importCommand}
                placeholder='e.g. agy plugin install notion@antigravity-plugins-partners  OR  claude mcp add --transport http linear https://mcp.linear.app/mcp'
                onChange={(e) => setImportCommand(e.target.value)}
                onKeyDown={(e) => {
                  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                    e.preventDefault();
                    runImportCommand();
                  }
                }}
              />
              <button
                type="button"
                className="primary import-submit-btn"
                disabled={busy || !importCommand.trim()}
                onClick={() => runImportCommand()}
              >
                {busyAction === "import" ? (
                  <>
                    <Spinner size={14} /> Parsing ({elapsedMs}ms)…
                  </>
                ) : (
                  "Parse & Auto-Fill"
                )}
              </button>
            </div>

            {importResult && importResult.candidates.length > 0 ? (
              <div className="callout success callout-anim" style={{ marginTop: "12px" }}>
                <div className="callout-title" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "8px", flexWrap: "wrap" }}>
                  <span>
                    ✓ Resolved via <strong>{importResult.sourceLabel}</strong>{" "}
                    <code style={{ fontSize: "11px" }}>{importResult.detectedFormat}</code>
                  </span>
                  <div className="row" style={{ gap: "8px", flexWrap: "wrap" }}>
                    {canQuickRegisterImported ? (
                      <>
                        <button
                          type="button"
                          className="btn-ghost compact"
                          disabled={busy}
                          onClick={() => changeStep(1)}
                        >
                          Customize Vault (Step 02) →
                        </button>
                        <button
                          type="button"
                          className="small primary"
                          disabled={busy}
                          onClick={() => void save()}
                        >
                          {busyAction === "save" ? (
                            <>
                              <Spinner size={12} tone="dark" /> Registering ({elapsedMs}ms)…
                            </>
                          ) : kind === "oauth2" && grant === "authorization_code" ? (
                            "⚡ One-Click Register & Authorize OAuth →"
                          ) : (
                            "⚡ One-Click Register Upstream →"
                          )}
                        </button>
                      </>
                    ) : (
                      <button
                        type="button"
                        className="small primary"
                        disabled={busy}
                        onClick={() => changeStep(1)}
                      >
                        Enter Secret in Step 02 (Credential Vault) →
                      </button>
                    )}
                  </div>
                </div>

                {importResult.candidates.length > 1 ? (
                  <div style={{ marginTop: "8px", display: "flex", gap: "6px", flexWrap: "wrap", alignItems: "center" }}>
                    <small className="muted">Bundle contains {importResult.candidates.length} MCP servers:</small>
                    {importResult.candidates.map((cand, idx) => (
                      <button
                        key={cand.input.slug + idx}
                        type="button"
                        className={`chip-btn ${selectedImportIdx === idx ? "active" : ""}`}
                        onClick={() => applyImportedCandidate(cand, idx)}
                      >
                        {cand.input.name} (<code>{cand.input.slug}</code>)
                      </button>
                    ))}
                  </div>
                ) : null}

                {importResult.candidates[selectedImportIdx]?.notes.length ? (
                  <ul className="import-notes-list">
                    {importResult.candidates[selectedImportIdx].notes.map((n, i) => (
                      <li key={i}>{n}</li>
                    ))}
                  </ul>
                ) : null}

                {importResult.candidates[selectedImportIdx]?.warnings.length ? (
                  <ul className="import-warnings-list">
                    {importResult.candidates[selectedImportIdx].warnings.map((w, i) => (
                      <li key={i}>⚠ {w}</li>
                    ))}
                  </ul>
                ) : null}
              </div>
            ) : null}
          </div>

          <div className="form-section">
            <div className="section-header">
              <h3>Server Identity</h3>
              <p>Give this upstream a human-friendly display name and a URL-safe slug for local CLI routing.</p>
            </div>
            <div className="two">
              <label className="field">
                <div className="field-label-row">
                  <span>Display Name *</span>
                  <small className="field-hint">1–80 chars</small>
                </div>
                <input
                  value={name}
                  placeholder="e.g. GitHub Copilot MCP"
                  onChange={(event) => updateName(event.target.value)}
                  autoFocus
                />
              </label>
              <label className="field">
                <div className="field-label-row">
                  <span>Route Slug *</span>
                  {slug && !slugValid ? (
                    <small className="field-error">Use lowercase a-z, 0-9, and single hyphens</small>
                  ) : (
                    <small className="field-hint">Lowercase & hyphens</small>
                  )}
                </div>
                <input
                  value={slug}
                  placeholder="e.g. github"
                  className={slug && !slugValid ? "invalid" : ""}
                  onChange={(event) => {
                    setSlugTouched(true);
                    setSlug(event.target.value.toLowerCase());
                  }}
                />
              </label>
            </div>

            <div className="route-preview-box">
              <div className="route-preview-item">
                <span className="badge-tag">Scoped CLI Endpoint</span>
                <code>
                  {publicBaseUrl}/mcp/{slug || "<slug>"}
                </code>
                <span className="muted">Exposes original tool names (recommended per CLI)</span>
              </div>
              <div className="route-preview-item">
                <span className="badge-tag subtle">Aggregated Endpoint</span>
                <code>{publicBaseUrl}/mcp</code>
                <span className="muted">
                  Namespaces tools as <code>{slug || "<slug>"}__tool_name</code>
                </span>
              </div>
            </div>
          </div>

          <div className="form-section">
            <div className="section-header">
              <h3>Remote Upstream Endpoint</h3>
              <p>Specify the remote MCP server URL and transport protocol.</p>
            </div>
            <label className="field">
              <div className="field-label-row">
                <span>Remote MCP Endpoint URL *</span>
                {url && !urlValid ? (
                  <small className="field-error">Must start with http:// or https://</small>
                ) : (
                  <small className="field-hint">HTTPS recommended</small>
                )}
              </div>
              <input
                value={url}
                placeholder="https://api.example.com/mcp"
                className={url && !urlValid ? "invalid" : ""}
                onChange={(event) => setUrl(event.target.value)}
              />
            </label>

            <div className="transport-grid">
              <button
                type="button"
                className={`choice-card ${transport === "streamable-http" ? "selected" : ""}`}
                onClick={() => setTransport("streamable-http")}
              >
                <div className="choice-card-head">
                  <strong>Streamable HTTP</strong>
                  <span className="badge-pill">Recommended</span>
                </div>
                <p>Modern MCP single-endpoint transport supporting JSON-RPC POST and event-stream responses.</p>
              </button>
              <button
                type="button"
                className={`choice-card ${transport === "sse" ? "selected" : ""}`}
                onClick={() => setTransport("sse")}
              >
                <div className="choice-card-head">
                  <strong>Legacy HTTP + SSE</strong>
                  <span className="badge-pill muted">2024-11-05</span>
                </div>
                <p>Opens a persistent GET event stream and dispatches JSON-RPC requests to the announced endpoint.</p>
              </button>
            </div>

            <div className="toggle-row">
              <label className="switch-label">
                <input
                  type="checkbox"
                  checked={enabled}
                  onChange={(event) => setEnabled(event.target.checked)}
                />
                <span className="switch-track" />
                <span>
                  <strong>Enable upstream immediately</strong>
                  <small>When enabled, the proxy maintains the connection and exposes its tools to local clients.</small>
                </span>
              </label>
            </div>
          </div>
        </div>
      ) : null}

      {/* STEP 1: CREDENTIAL VAULT */}
      {step === 1 ? (
        <div key="step-1" className={`wizard-body step-anim-${stepDir}`}>
          <div className="form-section">
            <div className="section-header">
              <h3>Authentication Strategy</h3>
              <p>
                Credentials entered here are encrypted with <strong>AES-256-GCM</strong> inside the container volume and
                never exposed to local CLI config files.
              </p>
            </div>

            <div className="auth-kind-grid">
              {AUTH_OPTIONS.map((opt) => (
                <button
                  key={opt.kind}
                  type="button"
                  className={`auth-kind-card ${kind === opt.kind ? "selected" : ""}`}
                  onClick={() => setKind(opt.kind)}
                >
                  <div className="choice-card-head">
                    <strong>{opt.title}</strong>
                    <span className="badge-pill">{opt.badge}</span>
                  </div>
                  <p>{opt.desc}</p>
                </button>
              ))}
            </div>
          </div>

          {kind === "none" ? (
            <div className="callout subtle callout-anim">
              <div className="callout-title">Unauthenticated Upstream</div>
              <p>
                Loopback will connect to <code>{url || "the upstream endpoint"}</code> without adding an{" "}
                <code>Authorization</code> header.
              </p>
            </div>
          ) : null}

          {kind === "bearer" ? (
            <div className="form-section mode-switch-anim">
              <div className="section-header">
                <h3>Bearer Token / API Key</h3>
                <p>Sent as `Authorization: Bearer &lt;token&gt;` on every upstream MCP request.</p>
              </div>
              <label className="field">
                <div className="field-label-row">
                  <span>Secret Bearer Token {canKeepStoredBearer ? "" : "*"}</span>
                  {canKeepStoredBearer ? (
                    <span className="vault-badge">Stored in Vault · Leave blank to keep existing</span>
                  ) : null}
                </div>
                <div className="secret-input-wrap">
                  <input
                    type={showBearer ? "text" : "password"}
                    value={bearer}
                    placeholder={
                      canKeepStoredBearer ? "•••••••••••••••••••••••• (unchanged if blank)" : "Paste API key or JWT token…"
                    }
                    onChange={(event) => setBearer(event.target.value)}
                  />
                  <button
                    type="button"
                    className="secret-toggle"
                    onClick={() => setShowBearer((prev) => !prev)}
                  >
                    {showBearer ? "Hide" : "Show"}
                  </button>
                </div>
              </label>
            </div>
          ) : null}

          {kind === "basic" ? (
            <div className="form-section mode-switch-anim">
              <div className="section-header">
                <h3>HTTP Basic Credentials</h3>
                <p>Encoded as standard Base64 `username:password`.</p>
              </div>
              <div className="two">
                <label className="field">
                  <div className="field-label-row">
                    <span>Username *</span>
                  </div>
                  <input
                    value={username}
                    placeholder="Account username"
                    onChange={(event) => setUsername(event.target.value)}
                  />
                </label>
                <label className="field">
                  <div className="field-label-row">
                    <span>Password {canKeepStoredBasic ? "" : "*"}</span>
                    {canKeepStoredBasic ? <span className="vault-badge">Stored in Vault</span> : null}
                  </div>
                  <div className="secret-input-wrap">
                    <input
                      type={showPassword ? "text" : "password"}
                      value={password}
                      placeholder={canKeepStoredBasic ? "•••••••••••• (unchanged if blank)" : "Enter password"}
                      onChange={(event) => setPassword(event.target.value)}
                    />
                    <button
                      type="button"
                      className="secret-toggle"
                      onClick={() => setShowPassword((prev) => !prev)}
                    >
                      {showPassword ? "Hide" : "Show"}
                    </button>
                  </div>
                </label>
              </div>
              <label className="field">
                <div className="field-label-row">
                  <span>Expected Realm (Optional)</span>
                  <small className="field-hint">Compared against WWW-Authenticate challenge</small>
                </div>
                <input
                  value={realm}
                  onChange={(event) => setRealm(event.target.value)}
                  placeholder="e.g. mcp-protected"
                />
              </label>
            </div>
          ) : null}

          {kind === "headers" ? (
            <div className="form-section mode-switch-anim">
              <div className="section-header">
                <h3>Custom Request Headers</h3>
                <p>Configure one or more custom HTTP headers to attach to every upstream request.</p>
              </div>
              <div className="headers-list">
                {headers.map((header, index) => {
                  const isStored =
                    canKeepStoredHeaders &&
                    initial?.auth.headerNames.some((n) => n.toLowerCase() === header.name.trim().toLowerCase());
                  const showVal = Boolean(visibleHeaders[index]);
                  return (
                    <div className="header-row-card" key={index}>
                      <input
                        placeholder="Header-Name (e.g. X-API-Key)"
                        value={header.name}
                        onChange={(event) => updateHeader(index, { ...header, name: event.target.value })}
                      />
                      <div className="secret-input-wrap">
                        <input
                          placeholder={isStored ? "•••••••• (blank keeps stored value)" : "Header secret value"}
                          type={showVal ? "text" : "password"}
                          value={header.value}
                          onChange={(event) => updateHeader(index, { ...header, value: event.target.value })}
                        />
                        <button
                          type="button"
                          className="secret-toggle"
                          onClick={() =>
                            setVisibleHeaders((prev) => ({ ...prev, [index]: !prev[index] }))
                          }
                        >
                          {showVal ? "Hide" : "Show"}
                        </button>
                      </div>
                      <button
                        type="button"
                        className="btn-ghost danger-text"
                        onClick={() =>
                          setHeaders(
                            headers.length > 1
                              ? headers.filter((_, item) => item !== index)
                              : [{ name: "", value: "" }],
                          )
                        }
                      >
                        Remove
                      </button>
                    </div>
                  );
                })}
              </div>
              <button
                type="button"
                className="btn-ghost"
                onClick={() => setHeaders([...headers, { name: "", value: "" }])}
              >
                + Add Another Header
              </button>
            </div>
          ) : null}

          {kind === "oauth2" ? (
            <div className="form-section mode-switch-anim">
              <div className="discovery-bar">
                <div>
                  <strong>OAuth 2.0 Metadata Discovery</strong>
                  <p>
                    Probe <code>{url || "the endpoint"}</code> for <code>WWW-Authenticate</code> and{" "}
                    <code>.well-known</code> OAuth endpoints automatically.
                  </p>
                </div>
                <button
                  type="button"
                  className="btn"
                  disabled={!urlValid || busy}
                  onClick={discover}
                >
                  {busyAction === "discover" ? (
                    <>
                      <Spinner size={14} tone="amber" />
                      <span>Discovering ({elapsedMs}ms)…</span>
                    </>
                  ) : (
                    "Auto-Discover Endpoints"
                  )}
                </button>
              </div>

              {discovery ? (
                <div className={`callout callout-anim ${discovery.tokenEndpoint ? "ok" : "subtle"}`}>
                  <div className="callout-title">{discovery.message}</div>
                  <div className="discovery-meta-tags">
                    {discovery.authorizationEndpoint ? (
                      <span className="badge-tag">Authorization URL found</span>
                    ) : null}
                    {discovery.tokenEndpoint ? <span className="badge-tag">Token URL found</span> : null}
                    {discovery.registrationEndpoint ? (
                      <span className="badge-tag">Dynamic Client Registration supported</span>
                    ) : null}
                    {discovery.scopes.length ? (
                      <span className="badge-tag">Scopes: {discovery.scopes.join(", ")}</span>
                    ) : null}
                  </div>
                </div>
              ) : null}

              <div className="transport-grid" style={{ marginTop: 14 }}>
                <button
                  type="button"
                  className={`choice-card ${grant === "authorization_code" ? "selected" : ""}`}
                  onClick={() => setGrant("authorization_code")}
                >
                  <div className="choice-card-head">
                    <strong>Authorization Code + PKCE</strong>
                    <span className="badge-pill">Browser Login</span>
                  </div>
                  <p>Interactive browser sign-in with S256 PKCE and automatic background refresh_token rotation.</p>
                </button>
                <button
                  type="button"
                  className={`choice-card ${grant === "client_credentials" ? "selected" : ""}`}
                  onClick={() => setGrant("client_credentials")}
                >
                  <div className="choice-card-head">
                    <strong>Client Credentials</strong>
                    <span className="badge-pill">Machine-to-Machine</span>
                  </div>
                  <p>Headless token issuance using Client ID and Secret, renewed automatically before expiry.</p>
                </button>
              </div>

              <div className="two" style={{ marginTop: 14 }}>
                <label className="field">
                  <div className="field-label-row">
                    <span>Client ID {grant === "client_credentials" ? "*" : ""}</span>
                    {grant === "authorization_code" ? (
                      <small className="field-hint">Blank uses dynamic registration if supported</small>
                    ) : null}
                  </div>
                  <input
                    value={clientId}
                    placeholder={
                      grant === "authorization_code"
                        ? "Optional if server supports RFC 7591 registration"
                        : "OAuth Client ID"
                    }
                    onChange={(event) => setClientId(event.target.value)}
                  />
                </label>
                <label className="field">
                  <div className="field-label-row">
                    <span>Client Secret</span>
                    {canKeepStoredOAuthSecret ? <span className="vault-badge">Stored in Vault</span> : null}
                  </div>
                  <div className="secret-input-wrap">
                    <input
                      type={showClientSecret ? "text" : "password"}
                      value={clientSecret}
                      placeholder={
                        canKeepStoredOAuthSecret ? "•••••••••••• (unchanged if blank)" : "Optional for public PKCE clients"
                      }
                      onChange={(event) => setClientSecret(event.target.value)}
                    />
                    <button
                      type="button"
                      className="secret-toggle"
                      onClick={() => setShowClientSecret((prev) => !prev)}
                    >
                      {showClientSecret ? "Hide" : "Show"}
                    </button>
                  </div>
                </label>
                {grant === "authorization_code" ? (
                  <label className="field">
                    <div className="field-label-row">
                      <span>Authorize Endpoint URL</span>
                      <small className="field-hint">Auto-filled by discovery</small>
                    </div>
                    <input
                      value={authorizeUrl ?? ""}
                      placeholder="https://auth.example.com/authorize"
                      onChange={(event) => setAuthorizeUrl(event.target.value)}
                    />
                  </label>
                ) : null}
                <label className="field">
                  <div className="field-label-row">
                    <span>Token Endpoint URL {grant === "client_credentials" ? "*" : ""}</span>
                    <small className="field-hint">Auto-filled by discovery</small>
                  </div>
                  <input
                    value={tokenUrl}
                    placeholder="https://auth.example.com/oauth/token"
                    onChange={(event) => setTokenUrl(event.target.value)}
                  />
                </label>
              </div>

              <label className="field">
                <div className="field-label-row">
                  <span>OAuth Scopes</span>
                  <small className="field-hint">Space or comma separated</small>
                </div>
                <input
                  value={scopes}
                  placeholder="e.g. mcp:tools read:org"
                  onChange={(event) => setScopes(event.target.value)}
                />
              </label>

              <div className="collapsible-seed">
                <button
                  type="button"
                  className="btn-link"
                  onClick={() => setShowManualTokens((prev) => !prev)}
                >
                  {showManualTokens ? "▾ Hide pre-issued token seed fields" : "▸ Have pre-issued Access / Refresh tokens? Seed them manually"}
                </button>
                {showManualTokens ? (
                  <div className="two mode-switch-anim" style={{ marginTop: 10 }}>
                    <label className="field">
                      <div className="field-label-row">
                        <span>Initial Access Token</span>
                        {initial?.auth.oauth?.hasAccessToken && !originChanged ? (
                          <span className="vault-badge">Stored</span>
                        ) : null}
                      </div>
                      <input
                        type="password"
                        value={accessToken}
                        placeholder="Optional pre-issued access token"
                        onChange={(event) => setAccessToken(event.target.value)}
                      />
                    </label>
                    <label className="field">
                      <div className="field-label-row">
                        <span>Initial Refresh Token</span>
                        {initial?.auth.oauth?.hasRefreshToken && !originChanged ? (
                          <span className="vault-badge">Stored</span>
                        ) : null}
                      </div>
                      <input
                        type="password"
                        value={refreshToken}
                        placeholder="Optional pre-issued refresh token"
                        onChange={(event) => setRefreshToken(event.target.value)}
                      />
                    </label>
                  </div>
                ) : null}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      {/* STEP 2: VERIFY & SAVE */}
      {step === 2 ? (
        <div key="step-2" className={`wizard-body step-anim-${stepDir}`}>
          <div className="form-section">
            <div className="section-header">
              <h3>Configuration Summary & Live Verification</h3>
              <p>
                Review your upstream configuration, run a live handshake probe (<code>initialize</code> +{" "}
                <code>tools/list</code>), and test-execute discovered tools before saving.
              </p>
            </div>

            <div className="summary-grid">
              <div className="summary-item">
                <span>Display Name</span>
                <strong>{name || "—"}</strong>
              </div>
              <div className="summary-item">
                <span>Local Route</span>
                <code>/mcp/{slug || "—"}</code>
              </div>
              <div className="summary-item">
                <span>Remote Endpoint</span>
                <code>{url || "—"}</code>
              </div>
              <div className="summary-item">
                <span>Transport & Auth</span>
                <strong>
                  {transport} · {kind.toUpperCase()}
                </strong>
              </div>
            </div>

            <div className="probe-action-box">
              <div>
                <strong>Test Upstream Handshake & Discover Tools</strong>
                <p className="muted">
                  Verifies network reachability, protocol negotiation, and credential validity without writing to disk.
                </p>
              </div>
              <button type="button" className="btn" disabled={busy || !identityReady} onClick={testConnection}>
                {busyAction === "probe" ? (
                  <>
                    <Spinner size={14} tone="amber" />
                    <span>Probing ({elapsedMs}ms)…</span>
                  </>
                ) : (
                  "Run Connection Test"
                )}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {/* Live Probe Result & Interactive Pre-Save Swagger Tool Sandbox */}
      {probe ? (
        <div className={`probe-result-card callout-anim ${probe.ok ? "ok" : "bad"}`} style={{ marginTop: 14 }}>
          <div className="probe-result-head" style={{ justifyContent: "space-between", flexWrap: "wrap" }}>
            <div className="row" style={{ gap: 10 }}>
              <span className={`status-pill ${probe.ok ? "connected" : "error"}`}>
                {probe.ok ? "Handshake Verified" : "Probe Failed"}
              </span>
              <strong>{probe.message}</strong>
            </div>
            {probe.ok && probe.tools?.length ? (
              <button
                type="button"
                className="btn compact"
                onClick={() => setShowProbeSandbox((prev) => !prev)}
              >
                {showProbeSandbox ? "▾ Hide Live Tool Sandbox" : `▸ Try Discovered Tools (${probe.tools.length})`}
              </button>
            ) : null}
          </div>

          {probe.tools?.length ? (
            <div className="probe-tools-preview">
              <div className="probe-tools-title">
                Discovered Tools ({probe.tools.length}) — Click any tool to test-call it live:
              </div>
              <div className="probe-tool-chips">
                {probe.tools.map((t) => {
                  const isSelected = selectedProbeTool === t.name && showProbeSandbox;
                  return (
                    <button
                      key={t.name}
                      type="button"
                      className={`tool-chip interactive ${isSelected ? "active" : ""}`}
                      title={t.description || t.name}
                      onClick={() => {
                        setSelectedProbeTool(t.name);
                        setShowProbeSandbox(true);
                      }}
                    >
                      <code>{t.name}</code>
                    </button>
                  );
                })}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      {/* Collapsible Swagger-style Tool Execution Sandbox right after Connection Test */}
      {probe?.ok && probe.tools?.length ? (
        <div className={`collapsible-drawer ${showProbeSandbox ? "open" : ""}`}>
          <div className="collapsible-inner">
            <div className="probe-sandbox-wrap" style={{ marginTop: 14 }}>
              <div className="probe-sandbox-head">
                <div>
                  <strong>Live Probe Tool Sandbox (Swagger Mode)</strong>
                  <span className="muted" style={{ marginLeft: 10, fontSize: 12 }}>
                    Execute <code>tools/call</code> against <code>{url}</code> using current form credentials before saving
                  </span>
                </div>
                <span className="badge-pill">Ephemeral Session</span>
              </div>
              <ToolBench
                compact
                tools={probe.tools}
                slug={slug.trim() || "probe"}
                probeInput={payload()}
                initialToolName={selectedProbeTool}
                onNotify={onNotify ?? onSaved}
              />
            </div>
          </div>
        </div>
      ) : null}

      {/* Real-Time Terminal Gadget during/after Connection Test */}
      {(busyAction === "probe" || probe || probeEntries.length > 0) ? (
        <div className="callout-anim" style={{ marginTop: 14 }}>
          <LiveTerminalGadget
            entries={probeEntries}
            title={`probe-stream — ${slug.trim() || "ephemeral"}`}
            busy={busyAction === "probe"}
            busyText={busyLabel || "Probing upstream…"}
            onClear={onClearActivity}
            onNotify={onNotify ?? onSaved}
            maxHeight={240}
          />
        </div>
      ) : null}

      {error ? (
        <div className="callout bad callout-anim" style={{ marginTop: 12 }}>
          <div className="callout-title">Validation / Connection Error</div>
          <p>{error}</p>
        </div>
      ) : null}

      {/* Persistent Wizard Footer */}
      <div className="wizard-footer">
        <div className="row">
          {step > 0 ? (
            <button type="button" className="btn-ghost" disabled={busy} onClick={() => changeStep(step - 1)}>
              {t("form.prevStep")}
            </button>
          ) : (
            <button
              type="button"
              className="btn-ghost"
              disabled={busy}
              onClick={() => navigate(initial ? `/servers/${initial.id}` : "/")}
            >
              {t("common.cancel")}
            </button>
          )}
        </div>

        <div className="row">
          {busy && busyLabel ? (
            <span className="busy-indicator">
              <Spinner size={13} tone="amber" />
              <span>
                {busyLabel} ({elapsedMs}ms)
              </span>
            </span>
          ) : null}
          <button
            type="button"
            className="btn-ghost"
            disabled={busy || !identityReady}
            onClick={testConnection}
          >
            {busyAction === "probe" ? (
              <>
                <Spinner size={13} tone="amber" />
                <span>{t("form.testingMs", { ms: elapsedMs })}</span>
              </>
            ) : (
              t("form.testConnection")
            )}
          </button>
          {step < 2 ? (
            <button
              type="button"
              className="btn"
              disabled={busy || (step === 0 && !identityReady)}
              onClick={() => changeStep(step + 1)}
            >
              {t("form.continueStep")}
            </button>
          ) : null}
          {(step === 2 || Boolean(initial)) ? (
            <button type="button" className="btn primary" disabled={busy || !identityReady} onClick={save}>
              {busyAction === "save" ? (
                <>
                  <Spinner size={14} tone="dark" />
                  <span>{busyLabel || t("common.save")}</span>
                </>
              ) : kind === "oauth2" && grant === "authorization_code" && !initial?.auth.oauth?.hasAccessToken ? (
                t("form.saveAndAuthorize")
              ) : initial ? (
                t("form.saveChanges")
              ) : (
                t("form.saveAndConnect")
              )}
            </button>
          ) : null}
        </div>
      </div>
    </div>
  );
}

function blank(value: string): string | null {
  return value.trim() ? value : null;
}

function slugify(value: string) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
}

function isValidSlug(slug: string): boolean {
  if (!slug || slug.length > 48) return false;
  return /^[a-z0-9]+(-[a-z0-9]+)*$/.test(slug);
}

function isValidHttpUrl(raw: string): boolean {
  try {
    const parsed = new URL(raw.trim());
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

function extractOrigin(raw: string): string {
  try {
    const parsed = new URL(raw.trim());
    return parsed.origin;
  } catch {
    return "";
  }
}

function isSameOrigin(a: string, b: string): boolean {
  const oa = extractOrigin(a);
  const ob = extractOrigin(b);
  return Boolean(oa && ob && oa === ob);
}
