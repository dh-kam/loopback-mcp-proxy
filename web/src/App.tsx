import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Link,
  NavLink,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";
import { api, loadToken, saveToken, wsUrl } from "./api";
import { ToolBench } from "./bench";
import { ServerForm } from "./form";
import { LanguageSwitcher, useI18n } from "./i18n";
import { RequiredAlertIcon, ServiceIcon } from "./icons";
import { ThemeSwitcher, useTheme } from "./theme";
import type {
  ActivityEntry,
  ConnState,
  ImportedCandidate,
  ImportResponse,
  Meta,
  ProbeResult,
  RegistryEntry,
  ServerInput,
  ServerView,
} from "./types";
import { CodeViewer, CopyButton, LiveTerminalGadget, Spinner, useElapsedTimer } from "./ui";

export function App() {
  const [meta, setMeta] = useState<Meta | null>(null);
  const [token, setToken] = useState(loadToken());
  const [ready, setReady] = useState(false);
  const [toast, setToast] = useState<string>("");
  const [toastTick, setToastTick] = useState(0);
  const { servers, activity, connected, clearActivity } = useLive(
    Boolean(meta) && (!meta?.authRequired || Boolean(token)),
  );

  const notify = useCallback((message: string) => {
    setToast(message);
    setToastTick((prev) => prev + 1);
  }, []);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(""), 3400);
    return () => window.clearTimeout(id);
  }, [toast, toastTick]);

  useEffect(() => {
    api<Meta>("/api/meta")
      .then(setMeta)
      .catch(() => setMeta(null));
  }, []);

  useEffect(() => {
    const onDenied = () => {
      saveToken("");
      setToken("");
    };
    window.addEventListener("admin-unauthorized", onDenied);
    return () => window.removeEventListener("admin-unauthorized", onDenied);
  }, []);

  useEffect(() => {
    if (!meta) return;
    if (meta.authRequired && !token) {
      setReady(true);
      return;
    }
    api<ServerView[]>("/api/servers")
      .then(() => setReady(true))
      .catch(() => setReady(true));
  }, [meta, token]);

  const { t } = useI18n();

  if (!meta || !ready) {
    return (
      <main className="gate-wrap">
        <div className="gate-card route-stage">
          <div className="brand-badge">LOOPBACK VAULT</div>
          <h1>Loopback</h1>
          <div className="row" style={{ gap: 10, marginTop: 10 }}>
            <Spinner size={18} tone="amber" />
            <p className="muted" style={{ margin: 0 }}>
              {t("common.loading")}
            </p>
          </div>
        </div>
      </main>
    );
  }
  if (meta.authRequired && !token) {
    return <Gate onSubmit={setToken} />;
  }

  return (
    <div className="shell">
      <header className="topbar">
        <div className="brand-group">
          <Link to="/" className="brand-logo">
            <span className="brand-icon">◈</span>
            <div>
              <h1>
                Loop<span>back</span>
              </h1>
              <p>{t("topbar.subtitle")}</p>
            </div>
          </Link>
          <span className="security-pill" title={t("topbar.vaultBadgeTitle")}>
            {t("topbar.vaultBadge")}
          </span>
        </div>

        <div className="topbar-actions">
          <ThemeSwitcher
            onSwitched={(th) => notify(`Theme: ${th.name}`)}
          />

          <LanguageSwitcher
            onSwitched={(loc) => notify(`Language: ${loc.nativeLabel} (${loc.label})`)}
          />

          <div className="endpoint-pill">
            <span className="muted">{t("topbar.aggregatedMcp")}</span>
            <code>{meta.mcpUrl}</code>
            <CopyButton
              text={meta.mcpUrl}
              label={t("common.copy")}
              copiedLabel={t("common.copied")}
              className="copy-hint"
              onCopied={() => notify(`Copied ${meta.mcpUrl}`)}
            />
          </div>

          <div className="live-pill">
            {connected ? (
              <i className="lamp on" />
            ) : (
              <Spinner size={11} tone="amber" />
            )}
            <span>{connected ? t("topbar.liveSync") : t("topbar.reconnecting")}</span>
          </div>

          {meta.authRequired ? (
            <button
              type="button"
              className="btn-ghost compact"
              onClick={() => {
                saveToken("");
                setToken("");
              }}
            >
              {t("topbar.lock")}
            </button>
          ) : null}
        </div>
      </header>

      <Frame
        servers={servers}
        activity={activity}
        onClearActivity={clearActivity}
        meta={meta}
        onNotify={notify}
      />

      {toast ? (
        <div
          key={`toast-${toastTick}`}
          className={`toast-banner ${toast.startsWith("?!") || toast.startsWith("⚠") ? "warning" : ""}`}
          role="status"
        >
          <span>
            {toast.startsWith("?!") || toast.startsWith("⚠") ? (
              <RequiredAlertIcon size={13} />
            ) : (
              "✓"
            )}
          </span>{" "}
          {toast.replace(/^(\?!|⚠)\s*/, "")}
        </div>
      ) : null}
    </div>
  );
}

function Gate({ onSubmit }: { onSubmit: (token: string) => void }) {
  const { t } = useI18n();
  const [value, setValue] = useState("");
  const [unlocking, setUnlocking] = useState(false);

  return (
    <main className="gate-wrap">
      <form
        className="gate-card route-stage"
        onSubmit={(event) => {
          event.preventDefault();
          if (!value.trim()) return;
          setUnlocking(true);
          window.setTimeout(() => {
            saveToken(value.trim());
            onSubmit(value.trim());
          }, 180);
        }}
      >
        <div className="brand-badge">PROTECTED CONSOLE</div>
        <h1>{t("gate.title")}</h1>
        <p className="muted">
          {t("gate.desc")}
        </p>
        <label className="field" style={{ marginTop: 16 }}>
          <span>Administrator Bearer Token</span>
          <input
            value={value}
            type="password"
            placeholder={t("gate.placeholder")}
            onChange={(event) => setValue(event.target.value)}
            autoFocus
          />
        </label>
        <div className="row" style={{ marginTop: 16 }}>
          <button className="btn primary" type="submit" disabled={unlocking} style={{ width: "100%" }}>
            {unlocking ? (
              <>
                <Spinner size={14} tone="dark" />
                <span>{t("gate.unlocking")}</span>
              </>
            ) : (
              t("gate.unlock")
            )}
          </button>
        </div>
      </form>
    </main>
  );
}

function Frame({
  servers,
  activity,
  onClearActivity,
  meta,
  onNotify,
}: {
  servers: ServerView[];
  activity: ActivityEntry[];
  onClearActivity: () => void;
  meta: Meta;
  onNotify: (msg: string) => void;
}) {
  const location = useLocation();
  const navigate = useNavigate();
  const { t, locales, setLang } = useI18n();
  const { themes, setThemeId, setStudioOpen } = useTheme();
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "connected" | "issues">("all");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteSearch, setPaletteSearch] = useState("");
  const [vaultModalOpen, setVaultModalOpen] = useState(false);
  const [vaultTab, setVaultTab] = useState<"export" | "import">("export");
  const [vaultPassphrase, setVaultPassphrase] = useState("");
  const [vaultImportJson, setVaultImportJson] = useState("");
  const [vaultBusy, setVaultBusy] = useState(false);
  const [vaultError, setVaultError] = useState("");
  const vaultElapsedMs = useElapsedTimer(vaultBusy);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((prev) => !prev);
      } else if (e.key === "Escape") {
        setPaletteOpen(false);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const paletteItems = useMemo(() => {
    const q = paletteSearch.trim().toLowerCase();
    const items: Array<{
      id: string;
      badge: string;
      title: string;
      subtitle: string;
      action: () => void;
    }> = [
      {
        id: "act-new",
        badge: "ACTION",
        title: t("sidebar.addUpstream"),
        subtitle: "Open the 3-step setup & CLI import wizard",
        action: () => navigate("/new"),
      },
      {
        id: "act-vault",
        badge: "VAULT",
        title: `🔒 ${t("sidebar.vaultBackupTitle")}`,
        subtitle: "Export or restore passphrase-sealed AES-256-GCM server bundle",
        action: () => setVaultModalOpen(true),
      },
      ...locales.map((loc) => ({
        id: `lang-${loc.code}`,
        badge: "I18N",
        title: `🌐 Language: ${loc.nativeLabel} (${loc.label})`,
        subtitle: `${t("topbar.langTitle")} → ${loc.code.toUpperCase()}`,
        action: () => {
          setLang(loc.code);
          onNotify(`Language: ${loc.nativeLabel} (${loc.label})`);
        },
      })),
      {
        id: "theme-studio",
        badge: "THEME",
        title: "🎨 Open Theme & Plugin Studio",
        subtitle: "Install or author custom .theme.json plugins",
        action: () => setStudioOpen(true),
      },
      ...themes.map((th) => ({
        id: `theme-${th.id}`,
        badge: "THEME",
        title: `🎨 Theme: ${th.name}`,
        subtitle: th.description || `${th.appearance} mode theme`,
        action: () => {
          setThemeId(th.id);
          onNotify(`Theme: ${th.name}`);
        },
      })),
    ];
    for (const s of servers) {
      items.push({
        id: `srv-${s.id}`,
        badge: "UPSTREAM",
        title: `${s.name} (/mcp/${s.slug})`,
        subtitle: `${s.state} · ${s.toolCount} tools · ${s.url}`,
        action: () => navigate(`/servers/${s.id}`),
      });
      for (const toolItem of s.tools) {
        items.push({
          id: `tool-${s.id}-${toolItem.name}`,
          badge: `TOOL · ${s.slug}`,
          title: `${s.slug}__${toolItem.name}`,
          subtitle: toolItem.description || `Execute ${toolItem.name} on ${s.name}`,
          action: () =>
            navigate(`/servers/${s.id}?tool=${encodeURIComponent(toolItem.name)}`),
        });
      }
    }
    if (!q) return items.slice(0, 18);
    return items
      .filter(
        (it) =>
          it.title.toLowerCase().includes(q) ||
          it.subtitle.toLowerCase().includes(q) ||
          it.badge.toLowerCase().includes(q),
      )
      .slice(0, 24);
  }, [paletteSearch, servers, navigate, t, locales, setLang, themes, setThemeId, setStudioOpen, onNotify]);

  async function handleVaultExport() {
    setVaultBusy(true);
    setVaultError("");
    try {
      const bundle = await api<Record<string, unknown>>("/api/vault/export", {
        method: "POST",
        body: JSON.stringify({ passphrase: vaultPassphrase }),
      });
      const blob = new Blob([JSON.stringify(bundle, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `loopback-vault-backup-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
      onNotify("Exported encrypted vault backup bundle");
      setVaultModalOpen(false);
      setVaultPassphrase("");
    } catch (err) {
      setVaultError(err instanceof Error ? err.message : "Vault export failed");
    } finally {
      setVaultBusy(false);
    }
  }

  async function handleVaultImport() {
    setVaultBusy(true);
    setVaultError("");
    try {
      const parsed = JSON.parse(vaultImportJson);
      const imported = await api<ServerView[]>("/api/vault/import", {
        method: "POST",
        body: JSON.stringify({
          passphrase: vaultPassphrase,
          bundle: parsed,
        }),
      });
      onNotify(`Restored ${imported.length} upstream(s) into encrypted vault`);
      setVaultModalOpen(false);
      setVaultPassphrase("");
      setVaultImportJson("");
    } catch (err) {
      setVaultError(err instanceof Error ? err.message : "Vault import failed");
    } finally {
      setVaultBusy(false);
    }
  }

  const visible = useMemo(() => {
    return servers.filter((server) => {
      if (statusFilter === "connected" && server.state !== "connected") return false;
      if (
        statusFilter === "issues" &&
        server.state !== "error" &&
        server.state !== "authRequired"
      ) {
        return false;
      }
      const hay = `${server.name} ${server.slug} ${server.url} ${server.auth.kind}`.toLowerCase();
      return hay.includes(query.toLowerCase());
    });
  }, [servers, query, statusFilter]);

  const issueCount = useMemo(
    () =>
      servers.filter((s) => s.state === "error" || s.state === "authRequired")
        .length,
    [servers],
  );

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="sidebar-nav">
          <Link
            to="/"
            className={`nav-overview ${location.pathname === "/" ? "active" : ""}`}
          >
            <span>⊞ {t("sidebar.overview")}</span>
            <span className="nav-count">{servers.length}</span>
          </Link>
          <Link to="/new" className="btn primary full-width">
            {t("sidebar.addUpstream")}
          </Link>
          <div className="row" style={{ gap: 6, marginTop: 4 }}>
            <button
              type="button"
              className="btn-ghost compact"
              style={{ flex: 1 }}
              onClick={() => {
                setPaletteSearch("");
                setPaletteOpen(true);
              }}
              title="Search upstreams, tools, and actions (Cmd+K / Ctrl+K)"
            >
              ⌘K {t("sidebar.cmdPalette")}
            </button>
            <button
              type="button"
              className="btn-ghost compact"
              style={{ flex: 1 }}
              onClick={() => {
                setVaultError("");
                setVaultModalOpen(true);
              }}
              title="Export or import passphrase-encrypted vault backup"
            >
              🔒 {t("sidebar.vaultBackupTitle")}
            </button>
          </div>
        </div>

        <div className="sidebar-filter-box">
          <div className="list-head">
            <h2>{t("sidebar.upstreams")}</h2>
            <div className="filter-pills">
              <button
                type="button"
                className={statusFilter === "all" ? "on" : ""}
                onClick={() => setStatusFilter("all")}
              >
                {t("sidebar.filterAll")}
              </button>
              <button
                type="button"
                className={statusFilter === "connected" ? "on" : ""}
                onClick={() => setStatusFilter("connected")}
              >
                {t("sidebar.filterOnline")}
              </button>
              <button
                type="button"
                className={statusFilter === "issues" ? "on" : ""}
                onClick={() => setStatusFilter("issues")}
              >
                {t("sidebar.filterIssues")} {issueCount > 0 ? `(${issueCount})` : ""}
              </button>
            </div>
          </div>
          <input
            className="filter-input"
            placeholder={t("sidebar.searchPlaceholder")}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>

        <ul className="servers-list">
          {visible.map((server) => (
            <li key={server.id}>
              <NavLink to={`/servers/${server.id}`}>
                <ServiceIcon slug={server.slug} name={server.name} size={26} />
                <div className="server-item-body">
                  <div className="server-item-top">
                    <div
                      className="row"
                      style={{ gap: 8, minWidth: 0, flexWrap: "nowrap" }}
                    >
                      <strong className="truncate">{server.slug}</strong>
                      {server.state === "connecting" ? (
                        <Spinner size={10} tone="amber" />
                      ) : (
                        <i className={`lamp ${lampClass(server.state)}`} />
                      )}
                    </div>
                    <span className="auth-mini-tag">
                      {authBadgeLabel(server.auth.kind)}
                    </span>
                  </div>
                  <div className="server-item-sub">
                    <span className="truncate">{server.name}</span>
                    <span>
                      {server.state === "connected"
                        ? `${server.toolCount} tools`
                        : label(server.state)}
                    </span>
                  </div>
                </div>
              </NavLink>
            </li>
          ))}
          {!visible.length ? (
            <li className="sidebar-empty">
              {servers.length === 0
                ? t("sidebar.emptyFleet")
                : t("sidebar.noMatch")}
            </li>
          ) : null}
        </ul>
      </aside>

      <section className="workspace">
        <div key={location.pathname} className="route-stage">
          <Routes>
            <Route
              path="/"
              element={
                <Home
                  servers={servers}
                  activity={activity}
                  onClearActivity={onClearActivity}
                  meta={meta}
                  onNotify={onNotify}
                />
              }
            />
            <Route
              path="/new"
              element={
                <Editor
                  publicBaseUrl={meta.publicBaseUrl}
                  activity={activity}
                  onClearActivity={onClearActivity}
                  onNotify={onNotify}
                />
              }
            />
            <Route
              path="/servers/:id"
              element={
                <Detail
                  servers={servers}
                  activity={activity}
                  onClearActivity={onClearActivity}
                  meta={meta}
                  onNotify={onNotify}
                />
              }
            />
            <Route
              path="/servers/:id/edit"
              element={
                <Editor
                  servers={servers}
                  publicBaseUrl={meta.publicBaseUrl}
                  activity={activity}
                  onClearActivity={onClearActivity}
                  onNotify={onNotify}
                />
              }
            />
          </Routes>
        </div>
      </section>

      {paletteOpen ? (
        <div className="modal-backdrop" onClick={() => setPaletteOpen(false)}>
          <div
            className="modal-dialog"
            style={{ maxWidth: 620, width: "92vw", padding: 16 }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="row" style={{ justifyContent: "space-between", marginBottom: 10 }}>
              <strong>⌘K Command & Tool Palette</strong>
              <small className="muted">ESC to close</small>
            </div>
            <input
              autoFocus
              placeholder="Search servers, MCP tools (e.g. search, echo), or actions…"
              value={paletteSearch}
              onChange={(e) => setPaletteSearch(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && paletteItems[0]) {
                  e.preventDefault();
                  paletteItems[0].action();
                  setPaletteOpen(false);
                }
              }}
            />
            <div
              className="stack"
              style={{
                marginTop: 10,
                maxHeight: 340,
                overflowY: "auto",
                gap: 6,
              }}
            >
              {paletteItems.map((it) => (
                <button
                  key={it.id}
                  type="button"
                  className="btn-ghost"
                  style={{
                    justifyContent: "space-between",
                    textAlign: "left",
                    padding: "8px 10px",
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                  }}
                  onClick={() => {
                    it.action();
                    setPaletteOpen(false);
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div className="row" style={{ gap: 6 }}>
                      <span className="badge-tag subtle">{it.badge}</span>
                      <strong>{it.title}</strong>
                    </div>
                    <div
                      className="muted truncate"
                      style={{ fontSize: 12, marginTop: 2 }}
                    >
                      {it.subtitle}
                    </div>
                  </div>
                  <span className="muted">↵</span>
                </button>
              ))}
            </div>
          </div>
        </div>
      ) : null}

      {vaultModalOpen ? (
        <div
          className="modal-backdrop"
          onClick={() => !vaultBusy && setVaultModalOpen(false)}
        >
          <div
            className="modal-dialog"
            style={{ maxWidth: 520, width: "92vw" }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="row" style={{ justifyContent: "space-between", marginBottom: 8 }}>
              <h3>🔒 Encrypted Vault Backup</h3>
              <div className="segmented-tabs">
                <button
                  type="button"
                  className={vaultTab === "export" ? "active" : ""}
                  onClick={() => {
                    setVaultTab("export");
                    setVaultError("");
                  }}
                >
                  Export Bundle
                </button>
                <button
                  type="button"
                  className={vaultTab === "import" ? "active" : ""}
                  onClick={() => {
                    setVaultTab("import");
                    setVaultError("");
                  }}
                >
                  Restore Bundle
                </button>
              </div>
            </div>
            <p className="muted" style={{ fontSize: 13 }}>
              {vaultTab === "export"
                ? "Seal all registered upstreams, OAuth tokens, API keys, and tool policies into a portable AES-256-GCM bundle protected by your passphrase."
                : "Restore a previously exported Loopback vault bundle into this proxy instance."}
            </p>
            <label className="field" style={{ marginTop: 10 }}>
              <span>Bundle Passphrase (minimum 8 characters)</span>
              <input
                type="password"
                placeholder="Enter encryption passphrase…"
                value={vaultPassphrase}
                onChange={(e) => setVaultPassphrase(e.target.value)}
              />
            </label>
            {vaultTab === "import" ? (
              <label className="field" style={{ marginTop: 10 }}>
                <span>Vault Bundle JSON</span>
                <textarea
                  rows={5}
                  placeholder='Paste contents of loopback-vault-backup.json…'
                  value={vaultImportJson}
                  onChange={(e) => setVaultImportJson(e.target.value)}
                />
              </label>
            ) : null}
            {vaultError ? (
              <div className="callout bad callout-anim" style={{ marginTop: 10 }}>
                {vaultError}
              </div>
            ) : null}
            <div className="modal-actions" style={{ marginTop: 14 }}>
              <button
                type="button"
                className="btn-ghost"
                disabled={vaultBusy}
                onClick={() => setVaultModalOpen(false)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn primary"
                disabled={
                  vaultBusy ||
                  vaultPassphrase.trim().length < 8 ||
                  (vaultTab === "import" && !vaultImportJson.trim())
                }
                onClick={() =>
                  void (vaultTab === "export"
                    ? handleVaultExport()
                    : handleVaultImport())
                }
              >
                {vaultBusy ? (
                  <>
                    <Spinner size={13} tone="dark" />
                    <span>Working ({vaultElapsedMs}ms)…</span>
                  </>
                ) : vaultTab === "export" ? (
                  "⬇ Export Encrypted Bundle"
                ) : (
                  "⚡ Restore Vault Bundle"
                )}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function Home({
  servers,
  activity,
  onClearActivity,
  meta,
  onNotify,
}: {
  servers: ServerView[];
  activity: ActivityEntry[];
  onClearActivity: () => void;
  meta: Meta;
  onNotify: (msg: string) => void;
}) {
  const navigate = useNavigate();
  const { t } = useI18n();
  const [params] = useSearchParams();
  const oauthError = params.get("oauthError");
  const [clientTab, setClientTab] = useState<
    "claude" | "agy" | "codex" | "gemini" | "cursor" | "json" | "toml"
  >("claude");
  const [targetSlug, setTargetSlug] = useState<string>(
    servers.length > 1 ? "__batch__" : servers[0]?.slug ?? "__all__",
  );
  const [busyState, setBusyState] = useState<{
    id: string;
    action: "reconnect" | "authorize";
  } | null>(null);
  const [quickImportCmd, setQuickImportCmd] = useState("");
  const [quickImportRes, setQuickImportRes] = useState<ImportResponse | null>(null);
  const [quickImportIdx, setQuickImportIdx] = useState(0);
  const [quickImportError, setQuickImportError] = useState("");
  const [quickImportBusy, setQuickImportBusy] = useState<"parse" | "register" | null>(null);
  const [registry, setRegistry] = useState<RegistryEntry[]>([]);
  const [registryCategory, setRegistryCategory] = useState<
    "featured" | "all" | "productivity" | "developer" | "cloud" | "data" | "design"
  >("featured");
  const [registrySearch, setRegistrySearch] = useState("");
  const [installingSlug, setInstallingSlug] = useState<string | null>(null);
  const quickElapsedMs = useElapsedTimer(quickImportBusy !== null || installingSlug !== null);

  useEffect(() => {
    api<RegistryEntry[]>("/api/registry")
      .then(setRegistry)
      .catch(() => setRegistry([]));
  }, []);

  const filteredRegistry = useMemo(() => {
    const q = registrySearch.trim().toLowerCase();
    return registry.filter((item) => {
      if (q) {
        return (
          item.name.toLowerCase().includes(q) ||
          item.slug.toLowerCase().includes(q) ||
          item.provider.toLowerCase().includes(q) ||
          item.description.toLowerCase().includes(q) ||
          item.url.toLowerCase().includes(q)
        );
      }
      if (registryCategory === "featured") return item.featured;
      if (registryCategory === "all") return true;
      return item.category === registryCategory;
    });
  }, [registry, registryCategory, registrySearch]);

  useEffect(() => {
    if (
      targetSlug !== "__all__" &&
      targetSlug !== "__batch__" &&
      !servers.some((s) => s.slug === targetSlug)
    ) {
      setTargetSlug(servers.length > 1 ? "__batch__" : servers[0]?.slug ?? "__all__");
    }
  }, [servers, targetSlug]);

  const connectedCount = useMemo(
    () => servers.filter((s) => s.state === "connected").length,
    [servers],
  );
  const totalTools = useMemo(
    () =>
      servers
        .filter((s) => s.state === "connected")
        .reduce((acc, s) => acc + s.toolCount, 0),
    [servers],
  );

  const selectedEndpoint =
    targetSlug === "__all__" || targetSlug === "__batch__"
      ? `${meta.publicBaseUrl}/mcp`
      : `${meta.publicBaseUrl}/mcp/${targetSlug}`;
  const selectedName =
    targetSlug === "__all__" || targetSlug === "__batch__"
      ? "loopback"
      : targetSlug;

  const snippetObj = useMemo(() => {
    const isMulti =
      (targetSlug === "__batch__" || clientTab === "json") && servers.length > 0;
    if (clientTab === "cursor") {
      return {
        mcpServers: isMulti
          ? Object.fromEntries(
              servers.map((s) => [
                s.slug,
                { url: `${meta.publicBaseUrl}/mcp/${s.slug}` },
              ]),
            )
          : {
              [selectedName]: {
                url: selectedEndpoint,
              },
            },
      };
    }
    return {
      mcpServers: isMulti
        ? Object.fromEntries(
            servers.map((s) => [
              s.slug,
              { type: "http", url: `${meta.publicBaseUrl}/mcp/${s.slug}` },
            ]),
          )
        : {
            [selectedName]: {
              type: "http",
              url: selectedEndpoint,
            },
          },
    };
  }, [clientTab, selectedName, selectedEndpoint, targetSlug, servers, meta.publicBaseUrl]);

  const codexTomlText = useMemo(() => {
    if ((targetSlug === "__batch__" || targetSlug === "__all__") && servers.length > 0) {
      return servers
        .map(
          (s) =>
            `[mcp_servers.${s.slug}]\nurl = "${meta.publicBaseUrl}/mcp/${s.slug}"`,
        )
        .join("\n\n");
    }
    return `[mcp_servers.${selectedName}]\nurl = "${selectedEndpoint}"`;
  }, [targetSlug, servers, meta.publicBaseUrl, selectedName, selectedEndpoint]);

  const isBatchMode = targetSlug === "__batch__" && servers.length > 0;
  const claudeCmd = isBatchMode
    ? servers
        .map(
          (s) =>
            `claude mcp add --transport http ${s.slug} ${meta.publicBaseUrl}/mcp/${s.slug}`,
        )
        .join("\n")
    : `claude mcp add --transport http ${selectedName} ${selectedEndpoint}`;
  const agyCmd = isBatchMode
    ? servers
        .map(
          (s) =>
            `agy mcp add --transport http ${s.slug} ${meta.publicBaseUrl}/mcp/${s.slug}`,
        )
        .join("\n")
    : `agy mcp add --transport http ${selectedName} ${selectedEndpoint}`;
  const codexCmd = isBatchMode
    ? servers
        .map(
          (s) =>
            `codex mcp add ${s.slug} --url ${meta.publicBaseUrl}/mcp/${s.slug}`,
        )
        .join("\n")
    : `codex mcp add ${selectedName} --url ${selectedEndpoint}`;
  const geminiCmd = isBatchMode
    ? servers
        .map(
          (s) =>
            `gemini mcp add --transport http ${s.slug} ${meta.publicBaseUrl}/mcp/${s.slug}`,
        )
        .join("\n")
    : `gemini mcp add --transport http ${selectedName} ${selectedEndpoint}`;

  const activeCliCmd =
    clientTab === "claude"
      ? claudeCmd
      : clientTab === "agy"
        ? agyCmd
        : clientTab === "codex"
          ? codexCmd
          : clientTab === "gemini"
            ? geminiCmd
            : clientTab === "toml"
              ? codexTomlText
              : "";

  function downloadActiveSnippet() {
    const content = activeCliCmd
      ? activeCliCmd + "\n"
      : JSON.stringify(snippetObj, null, 2) + "\n";
    const filename =
      clientTab === "toml"
        ? "config.toml"
        : clientTab === "cursor" || clientTab === "json"
          ? "mcp.json"
          : `loopback-${clientTab}-setup.sh`;
    const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
    onNotify(`Downloaded ${filename}`);
  }

  function candidateNeedsSecret(cand: ImportedCandidate): boolean {
    const auth = cand.input.auth;
    if (auth.kind === "bearer") return !auth.bearerToken?.trim();
    if (auth.kind === "basic") {
      return !auth.basicUsername?.trim() || !auth.basicPassword?.trim();
    }
    if (auth.kind === "headers") {
      return !(auth.headers ?? []).some((h) => h.name.trim() && h.value.trim());
    }
    if (auth.kind === "oauth2") {
      const oauth = auth.oauth;
      if (!oauth) return true;
      const grant = oauth.grant ?? "authorization_code";
      const authUrl = oauth.authorizeUrl || cand.discovery?.authorizationEndpoint || "";
      const tokUrl = oauth.tokenUrl || cand.discovery?.tokenEndpoint || "";
      if (grant === "authorization_code") {
        return !authUrl.trim() || !tokUrl.trim();
      }
      return !tokUrl.trim() || !oauth.clientId?.trim() || !oauth.clientSecret?.trim();
    }
    return false;
  }

  async function runQuickImport(customCmd?: string) {
    const raw = (customCmd ?? quickImportCmd).trim();
    if (!raw) {
      setQuickImportError(
        "Paste an external install command (claude, agy, codex, gemini, cursor) or JSON/TOML config first.",
      );
      return;
    }
    if (customCmd !== undefined) {
      setQuickImportCmd(customCmd);
    }
    setQuickImportBusy("parse");
    setQuickImportError("");
    try {
      const res = await api<ImportResponse>("/api/import/parse", {
        method: "POST",
        body: JSON.stringify({ command: raw, autoDiscover: true }),
      });
      setQuickImportRes(res);
      setQuickImportIdx(0);
      if (res.candidates.length > 0) {
        onNotify(`Parsed ${res.candidates[0].input.name} (${res.sourceLabel})`);
      }
    } catch (err) {
      setQuickImportError(
        err instanceof Error ? err.message : "Failed to parse install command",
      );
    } finally {
      setQuickImportBusy(null);
    }
  }

  async function quickRegisterCandidate(cand: ImportedCandidate) {
    setQuickImportBusy("register");
    setQuickImportError("");
    try {
      const payload: ServerInput = {
        ...cand.input,
        auth: {
          ...cand.input.auth,
          oauth: cand.input.auth.oauth
            ? {
                ...cand.input.auth.oauth,
                authorizeUrl:
                  cand.input.auth.oauth.authorizeUrl ||
                  cand.discovery?.authorizationEndpoint ||
                  undefined,
                tokenUrl:
                  cand.input.auth.oauth.tokenUrl ||
                  cand.discovery?.tokenEndpoint ||
                  "",
                scopes:
                  cand.input.auth.oauth.scopes?.length
                    ? cand.input.auth.oauth.scopes
                    : cand.discovery?.scopes ?? [],
              }
            : null,
        },
      };
      const saved = await api<ServerView>("/api/servers", {
        method: "POST",
        body: JSON.stringify(payload),
      });
      if (
        saved.auth.kind === "oauth2" &&
        saved.auth.oauth?.grant === "authorization_code" &&
        !saved.auth.oauth?.hasAccessToken
      ) {
        const started = await api<{ authorizeUrl: string }>(
          `/api/servers/${saved.id}/oauth/start`,
          { method: "POST" },
        );
        window.location.href = started.authorizeUrl;
        return;
      }
      onNotify(`Registered upstream ${saved.slug}`);
      setQuickImportRes(null);
      setQuickImportCmd("");
      navigate(`/servers/${saved.id}`);
    } catch (err) {
      setQuickImportError(
        err instanceof Error ? err.message : "Failed to register upstream",
      );
    } finally {
      setQuickImportBusy(null);
    }
  }

  async function installRegistryItem(item: RegistryEntry) {
    if (item.authKind === "bearer" || item.authKind === "headers" || item.authKind === "basic") {
      navigate(`/new?import=${encodeURIComponent(item.installCommand)}&step=1`);
      return;
    }
    setInstallingSlug(item.slug);
    setQuickImportError("");
    try {
      const parsed = await api<ImportResponse>("/api/import/parse", {
        method: "POST",
        body: JSON.stringify({ command: item.installCommand, autoDiscover: true }),
      });
      const cand = parsed.candidates[0];
      if (!cand) {
        throw new Error(`Could not resolve configuration for ${item.name}`);
      }
      if (candidateNeedsSecret(cand)) {
        navigate(`/new?import=${encodeURIComponent(item.installCommand)}&step=1`);
        return;
      }
      await quickRegisterCandidate(cand);
    } catch (err) {
      setQuickImportError(
        err instanceof Error ? err.message : `Failed to install ${item.name}`,
      );
    } finally {
      setInstallingSlug(null);
    }
  }

  async function quickReconnect(server: ServerView) {
    setBusyState({ id: server.id, action: "reconnect" });
    try {
      const updated = await api<ServerView>(
        `/api/servers/${server.id}/reconnect`,
        { method: "POST" },
      );
      onNotify(
        `${updated.slug}: ${updated.state === "connected" ? `connected (${updated.toolCount} tools)` : updated.message || updated.state}`,
      );
    } catch (err) {
      onNotify(err instanceof Error ? err.message : "Reconnect failed");
    } finally {
      setBusyState(null);
    }
  }

  async function quickAuthorize(server: ServerView) {
    setBusyState({ id: server.id, action: "authorize" });
    try {
      const started = await api<{ authorizeUrl: string }>(
        `/api/servers/${server.id}/oauth/start`,
        { method: "POST" },
      );
      window.location.href = started.authorizeUrl;
    } catch (err) {
      onNotify(err instanceof Error ? err.message : "OAuth start failed");
      setBusyState(null);
    }
  }

  return (
    <div className="dashboard">
      {oauthError ? (
        <div className="callout bad callout-anim">
          <div className="callout-title">OAuth Authorization Error</div>
          <p>{oauthError}</p>
        </div>
      ) : null}

      {/* Metrics Row */}
      <div className="metrics-grid">
        <div className="metric-card">
          <span className="metric-label">{t("sidebar.upstreams")}</span>
          <strong className="metric-value">{servers.length}</strong>
          <small className="muted">
            {t("home.statConnectedSub", {
              total: servers.length,
              enabled: servers.filter((s) => s.enabled).length,
            })}
          </small>
        </div>
        <div className="metric-card">
          <span className="metric-label">{t("home.statConnected")}</span>
          <strong className="metric-value accent-green">
            {connectedCount} <span className="metric-sub">/ {servers.length}</span>
          </strong>
          <small className="muted">
            {connectedCount === servers.filter((s) => s.enabled).length
              ? t("home.statAttentionOk")
              : t("home.statAttentionWarn")}
          </small>
        </div>
        <div className="metric-card">
          <span className="metric-label">{t("home.statTools")}</span>
          <strong className="metric-value accent-amber">{totalTools}</strong>
          <small className="muted">{t("home.statToolsSub")}</small>
        </div>
        <div className="metric-card">
          <span className="metric-label">{t("home.statVault")}</span>
          <strong className="metric-value" style={{ fontSize: 20 }}>
            Zero Host Leak
          </strong>
          <small className="muted">{t("home.statVaultSub")}</small>
        </div>
      </div>

      {/* Universal CLI & Plugin Compatibility Installer */}
      <div className="import-box" style={{ marginBottom: 20 }}>
        <div className="import-box-head">
          <div>
            <span className="import-badge">{t("home.importBadge")}</span>
            <h3>{t("home.importTitle")}</h3>
            <p>{t("home.importDesc")}</p>
          </div>
        </div>

        <div className="import-examples">
          <span className="muted" style={{ fontSize: "12px" }}>Quick presets:</span>
          <button
            type="button"
            className="chip-btn preset-with-icon"
            disabled={quickImportBusy !== null}
            onClick={() => void runQuickImport("agy plugin install notion@antigravity-plugins-partners")}
          >
            <ServiceIcon slug="notion" name="Notion" size={16} />
            <span>AGY Plugin (<code>notion</code>)</span>
          </button>
          <button
            type="button"
            className="chip-btn preset-with-icon"
            disabled={quickImportBusy !== null}
            onClick={() => void runQuickImport("claude mcp add --transport http linear https://mcp.linear.app/mcp")}
          >
            <ServiceIcon slug="linear" name="Linear" size={16} />
            <span>Claude HTTP (<code>linear</code>)</span>
          </button>
          <button
            type="button"
            className="chip-btn preset-with-icon"
            disabled={quickImportBusy !== null}
            onClick={() =>
              void runQuickImport(
                "codex mcp add github --url https://api.githubcopilot.com/mcp/ --bearer-token-env-var GITHUB_PERSONAL_ACCESS_TOKEN",
              )
            }
          >
            <ServiceIcon slug="github" name="GitHub" size={16} />
            <span>Codex CLI (<code>github</code>)</span>
          </button>
          <button
            type="button"
            className="chip-btn preset-with-icon"
            disabled={quickImportBusy !== null}
            onClick={() => void runQuickImport("gemini mcp add -t http context7 https://mcp.context7.com/mcp")}
          >
            <ServiceIcon slug="context7" name="Context7" size={16} />
            <span>Gemini CLI (<code>context7</code>)</span>
          </button>
          <button
            type="button"
            className="chip-btn preset-with-icon"
            disabled={quickImportBusy !== null}
            onClick={() =>
              void runQuickImport(
                "cursor://anysphere.cursor-deeplink/mcp/install?name=sentry&config=eyJ1cmwiOiJodHRwczovL21jcC5zZW50cnkuZGV2L21jcCJ9",
              )
            }
          >
            <ServiceIcon slug="sentry" name="Sentry" size={16} />
            <span>Cursor Deeplink (<code>sentry</code>)</span>
          </button>
          <button
            type="button"
            className="chip-btn preset-with-icon"
            disabled={quickImportBusy !== null}
            onClick={() => void runQuickImport("agy plugin install stripe")}
          >
            <ServiceIcon slug="stripe" name="Stripe" size={16} />
            <span>Stripe (<code>stripe</code>)</span>
          </button>
          <button
            type="button"
            className="chip-btn preset-with-icon"
            disabled={quickImportBusy !== null}
            onClick={() => void runQuickImport("claude mcp add --transport http supabase https://mcp.supabase.com/mcp")}
          >
            <ServiceIcon slug="supabase" name="Supabase" size={16} />
            <span>Supabase (<code>supabase</code>)</span>
          </button>
        </div>

        <div className="import-input-row">
          <textarea
            className="import-textarea"
            rows={2}
            value={quickImportCmd}
            placeholder={t("home.importPlaceholder")}
            onChange={(e) => setQuickImportCmd(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                e.preventDefault();
                void runQuickImport();
              }
            }}
          />
          <button
            type="button"
            className="primary import-submit-btn"
            disabled={quickImportBusy !== null || !quickImportCmd.trim()}
            onClick={() => void runQuickImport()}
          >
            {quickImportBusy === "parse" ? (
              <>
                <Spinner size={14} /> {t("home.importParsing", { ms: quickElapsedMs })}
              </>
            ) : (
              t("home.importBtn")
            )}
          </button>
        </div>

        {quickImportError ? (
          <div className="callout bad callout-anim" style={{ marginTop: 10 }}>
            <div className="callout-title">Import Parse Error</div>
            <p>{quickImportError}</p>
          </div>
        ) : null}

        {quickImportRes && quickImportRes.candidates.length > 0 ? (
          <div className="callout success callout-anim" style={{ marginTop: 12 }}>
            {(() => {
              const cand =
                quickImportRes.candidates[quickImportIdx] ??
                quickImportRes.candidates[0];
              const requiresSecret = candidateNeedsSecret(cand);
              return (
                <>
                  <div
                    className="callout-title"
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      gap: 8,
                      flexWrap: "wrap",
                    }}
                  >
                    <span>
                      ✓ Resolved <strong>{cand.input.name}</strong> (<code>/mcp/{cand.input.slug}</code>) via{" "}
                      <strong>{quickImportRes.sourceLabel}</strong>{" "}
                      <code style={{ fontSize: 11 }}>{quickImportRes.detectedFormat}</code>
                    </span>
                    <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                      <button
                        type="button"
                        className="btn-ghost compact"
                        disabled={quickImportBusy !== null}
                        onClick={() =>
                          navigate(
                            `/new?import=${encodeURIComponent(quickImportCmd.trim())}${requiresSecret ? "&step=1" : ""}`,
                          )
                        }
                      >
                        {requiresSecret
                          ? "Enter Secret in Wizard (Step 02) →"
                          : "Customize in Wizard →"}
                      </button>
                      {!requiresSecret ? (
                        <button
                          type="button"
                          className="btn primary compact"
                          disabled={quickImportBusy !== null}
                          onClick={() => void quickRegisterCandidate(cand)}
                        >
                          {quickImportBusy === "register" ? (
                            <>
                              <Spinner size={12} tone="dark" /> Registering ({quickElapsedMs}ms)…
                            </>
                          ) : cand.input.auth.kind === "oauth2" &&
                            (cand.input.auth.oauth?.grant ?? "authorization_code") ===
                              "authorization_code" ? (
                            "⚡ One-Click Register & Authorize OAuth →"
                          ) : (
                            "⚡ One-Click Register Upstream →"
                          )}
                        </button>
                      ) : null}
                      <button
                        type="button"
                        className="btn-link"
                        disabled={quickImportBusy !== null}
                        onClick={() => setQuickImportRes(null)}
                      >
                        Dismiss
                      </button>
                    </div>
                  </div>

                  {quickImportRes.candidates.length > 1 ? (
                    <div
                      style={{
                        marginTop: 8,
                        display: "flex",
                        gap: 6,
                        flexWrap: "wrap",
                        alignItems: "center",
                      }}
                    >
                      <small className="muted">
                        Bundle contains {quickImportRes.candidates.length} MCP servers:
                      </small>
                      {quickImportRes.candidates.map((item, idx) => (
                        <button
                          key={item.input.slug + idx}
                          type="button"
                          className={`chip-btn ${quickImportIdx === idx ? "active" : ""}`}
                          onClick={() => setQuickImportIdx(idx)}
                        >
                          {item.input.name} (<code>{item.input.slug}</code>)
                        </button>
                      ))}
                    </div>
                  ) : null}

                  {cand.notes.length ? (
                    <ul className="import-notes-list">
                      {cand.notes.map((n, i) => (
                        <li key={i}>{n}</li>
                      ))}
                    </ul>
                  ) : null}

                  {cand.warnings.length ? (
                    <ul className="import-warnings-list">
                      {cand.warnings.map((w, i) => (
                        <li key={i}>⚠ {w}</li>
                      ))}
                    </ul>
                  ) : null}
                </>
              );
            })()}
          </div>
        ) : null}

        {/* Verified MCP Service Registry */}
        {registry.length > 0 ? (
          <div className="registry-catalog">
            <div className="registry-toolbar">
              <div className="registry-tabs">
                {(
                  [
                    { id: "featured", label: "★ Featured" },
                    { id: "all", label: `All (${registry.length})` },
                    { id: "productivity", label: "Productivity & Docs" },
                    { id: "developer", label: "Developer & Code" },
                    { id: "cloud", label: "Cloud & Payments" },
                    { id: "data", label: "Data & Security" },
                    { id: "design", label: "Design & AI" },
                  ] as const
                ).map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    className={`chip-btn ${!registrySearch.trim() && registryCategory === tab.id ? "active" : ""}`}
                    onClick={() => {
                      setRegistrySearch("");
                      setRegistryCategory(tab.id);
                    }}
                  >
                    {tab.label}
                  </button>
                ))}
              </div>
              <input
                type="search"
                className="registry-search-input"
                value={registrySearch}
                placeholder="Search 35+ popular MCP services (Notion, Linear, GitHub, Jira, Sentry, Supabase…)"
                onChange={(e) => setRegistrySearch(e.target.value)}
              />
            </div>

            <div key={registryCategory + registrySearch} className="registry-grid mode-switch-anim">
              {filteredRegistry.map((item) => {
                const existing = servers.find((s) => s.slug === item.slug);
                const isInstalling = installingSlug === item.slug;
                const anyBusy = quickImportBusy !== null || installingSlug !== null;
                return (
                  <div key={item.slug} className={`registry-card ${existing ? "installed" : ""}`}>
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
                        <ServiceIcon slug={item.slug} name={item.name} size={32} />
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
                      <code className="registry-url" title={item.url}>
                        {item.url}
                      </code>
                      <div className="row" style={{ gap: 6 }}>
                        {existing ? (
                          <Link className="btn-ghost compact" to={`/servers/${existing.id}`}>
                            ✓ Installed (View) →
                          </Link>
                        ) : (
                          <>
                            <button
                              type="button"
                              className="btn-ghost compact"
                              disabled={anyBusy}
                              onClick={() =>
                                navigate(`/new?import=${encodeURIComponent(item.installCommand)}`)
                              }
                            >
                              Wizard
                            </button>
                            <button
                              type="button"
                              className="btn primary compact"
                              disabled={anyBusy}
                              onClick={() => void installRegistryItem(item)}
                            >
                              {isInstalling ? (
                                <>
                                  <Spinner size={11} tone="dark" /> ({quickElapsedMs}ms)…
                                </>
                              ) : item.authKind === "oauth2" ? (
                                "⚡ 1-Click + OAuth"
                              ) : item.authKind === "none" ? (
                                "⚡ 1-Click Install"
                              ) : (
                                "🔑 Set Key & Install"
                              )}
                            </button>
                          </>
                        )}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}
      </div>

      {/* Quick Connect CLI Hub */}
      <div className="hub-card">
        <div className="hub-head">
          <div>
            <h2>{t("home.cliHubTitle")}</h2>
            <p className="muted">{t("home.cliHubSubtitle")}</p>
          </div>
          <div className="hub-target-picker">
            <span>{t("home.cliScopeLabel")}</span>
            <select
              value={targetSlug}
              onChange={(event) => setTargetSlug(event.target.value)}
            >
              {servers.length > 0 ? (
                <option value="__batch__">
                  Batch All Scoped (/mcp/&#123;slug&#125; × {servers.length} servers)
                </option>
              ) : null}
              <option value="__all__">
                {t("home.cliScopeAggregated")}
              </option>
              {servers.map((s) => (
                <option key={s.id} value={s.slug}>
                  Scoped (/mcp/{s.slug} — {s.name})
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="hub-tabs-bar">
          <div className="segmented-tabs">
            <button
              type="button"
              className={clientTab === "claude" ? "active" : ""}
              onClick={() => setClientTab("claude")}
            >
              Claude Code
            </button>
            <button
              type="button"
              className={clientTab === "agy" ? "active" : ""}
              onClick={() => setClientTab("agy")}
            >
              AGY CLI
            </button>
            <button
              type="button"
              className={clientTab === "codex" ? "active" : ""}
              onClick={() => setClientTab("codex")}
            >
              Codex CLI
            </button>
            <button
              type="button"
              className={clientTab === "gemini" ? "active" : ""}
              onClick={() => setClientTab("gemini")}
            >
              Gemini CLI
            </button>
            <button
              type="button"
              className={clientTab === "cursor" ? "active" : ""}
              onClick={() => setClientTab("cursor")}
            >
              Cursor / Desktop
            </button>
            <button
              type="button"
              className={clientTab === "json" ? "active" : ""}
              onClick={() => setClientTab("json")}
            >
              Multi-Server JSON
            </button>
            <button
              type="button"
              className={clientTab === "toml" ? "active" : ""}
              onClick={() => setClientTab("toml")}
            >
              Codex TOML
            </button>
          </div>
          <div className="row" style={{ gap: 6 }}>
            {activeCliCmd ? (
              <CopyButton
                text={activeCliCmd}
                label={t("common.copy")}
                copiedLabel={t("common.copied")}
                className="btn compact"
                onCopied={() =>
                  onNotify(`Copied ${clientTab.toUpperCase()} setup snippet`)
                }
              />
            ) : null}
            <button
              type="button"
              className="btn compact ghost"
              onClick={downloadActiveSnippet}
              title="Download configuration file or shell script"
            >
              ⬇ Download
            </button>
          </div>
        </div>

        <div key={clientTab} className="mode-switch-anim">
          {activeCliCmd ? (
            <pre className="hub-code">{activeCliCmd}</pre>
          ) : (
            <CodeViewer
              data={snippetObj}
              title={clientTab === "cursor" ? "mcp.json (Cursor / Claude Desktop)" : "mcp.json (All Upstreams)"}
              defaultMode="json"
              maxHeight={220}
              onNotify={onNotify}
            />
          )}
        </div>
      </div>

      {/* Upstream Fleet Section */}
      <div className="fleet-section">
        <div className="fleet-head">
          <h2>{t("home.fleetTitle")}</h2>
          <Link className="btn primary" to="/new">
            {t("home.registerBtn")}
          </Link>
        </div>

        {!servers.length ? (
          <div className="empty-fleet">
            <strong>No upstream MCP servers configured yet</strong>
            <p>
              Register your first remote MCP server. Loopback will seal its OAuth tokens, Bearer keys, Basic passwords,
              or custom headers inside the encrypted SQLite vault and expose an unauthenticated local endpoint.
            </p>
            <Link className="btn primary" to="/new">
              Launch Upstream Setup Wizard →
            </Link>
          </div>
        ) : (
          <div className="fleet-grid">
            {servers.map((server) => {
              const isReconnecting =
                busyState?.id === server.id && busyState.action === "reconnect";
              const isAuthorizing =
                busyState?.id === server.id && busyState.action === "authorize";
              const isCardBusy = busyState?.id === server.id;

              return (
                <div key={server.id} className="fleet-card">
                  <div className="fleet-card-top">
                    <div
                      className="row"
                      style={{ gap: 10, minWidth: 0, flexWrap: "nowrap" }}
                    >
                      <ServiceIcon slug={server.slug} name={server.name} size={26} />
                      <Link to={`/servers/${server.id}`} className="fleet-title truncate">
                        {server.name}
                      </Link>
                    </div>
                    <span className={`status-pill ${server.state}`}>
                      {server.state === "connecting" || isReconnecting ? (
                        <Spinner size={10} tone="amber" />
                      ) : (
                        <i className={`lamp ${lampClass(server.state)}`} />
                      )}
                      {label(server.state)}
                    </span>
                  </div>

                  <div className="fleet-meta">
                    <div className="row" style={{ justifyContent: "space-between" }}>
                      <div>
                        <span className="muted">Local Route:</span>{" "}
                        <code>/mcp/{server.slug}</code>
                      </div>
                      <CopyButton
                        text={`${meta.publicBaseUrl}/mcp/${server.slug}`}
                        label="Copy Route"
                        className="inline-copy"
                        onCopied={() =>
                          onNotify(`Copied ${meta.publicBaseUrl}/mcp/${server.slug}`)
                        }
                      />
                    </div>
                    <div className="truncate">
                      <span className="muted">Upstream:</span>{" "}
                      <code>{server.url}</code>
                    </div>
                    <div>
                      <span className="muted">Vault Auth:</span>{" "}
                      <span>{describeAuth(server)}</span>
                    </div>
                  </div>

                  {server.message ? (
                    <div className="fleet-message">{server.message}</div>
                  ) : null}

                  <div className="fleet-card-foot">
                    <div className="row" style={{ gap: 6 }}>
                      <span className="tool-count-pill">
                        {server.exposedToolCount !== undefined &&
                        server.exposedToolCount < server.toolCount
                          ? `${server.exposedToolCount}/${server.toolCount} tools`
                          : `${server.toolCount} ${server.toolCount === 1 ? "tool" : "tools"}`}
                      </span>
                      {server.toolPolicy?.mode === "readOnly" ? (
                        <span className="badge-tag" title="Read-Only Shield active: mutating and destructive tools are blocked">
                          🛡️ Read-Only
                        </span>
                      ) : (server.toolPolicy?.deniedTools?.length ?? 0) > 0 ? (
                        <span className="badge-tag subtle">
                          🛡️ {server.toolPolicy?.deniedTools.length} blocked
                        </span>
                      ) : null}
                      {(server.totalCalls ?? 0) > 0 ? (
                        <span className="badge-tag subtle" title="Total MCP calls & last latency">
                          ⚡ {server.totalCalls} calls
                          {server.lastLatencyMs !== undefined && server.lastLatencyMs !== null
                            ? ` · ${server.lastLatencyMs}ms`
                            : ""}
                        </span>
                      ) : null}
                    </div>
                    <div className="row" style={{ gap: 6 }}>
                      {server.auth.kind === "oauth2" &&
                      server.auth.oauth?.grant === "authorization_code" &&
                      server.state === "authRequired" ? (
                        <button
                          type="button"
                          className="btn compact"
                          disabled={isCardBusy}
                          onClick={() => quickAuthorize(server)}
                        >
                          {isAuthorizing ? (
                            <>
                              <Spinner size={12} tone="amber" />
                              <span>Redirecting…</span>
                            </>
                          ) : (
                            "Authorize"
                          )}
                        </button>
                      ) : null}
                      <button
                        type="button"
                        className="btn-ghost compact"
                        disabled={isCardBusy}
                        onClick={() => quickReconnect(server)}
                      >
                        {isReconnecting ? (
                          <>
                            <Spinner size={12} tone="amber" />
                            <span>Syncing…</span>
                          </>
                        ) : (
                          "Reconnect"
                        )}
                      </button>
                      <Link
                        to={`/servers/${server.id}/edit`}
                        className="btn-ghost compact"
                      >
                        Edit
                      </Link>
                      <Link
                        to={`/servers/${server.id}`}
                        className="btn compact"
                      >
                        Inspect →
                      </Link>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Global Real-Time MCP Activity Stream */}
      <LiveTerminalGadget
        entries={activity}
        title="loopback-proxy — global mcp telemetry"
        busy={Boolean(busyState)}
        onClear={onClearActivity}
        onNotify={onNotify}
        maxHeight={280}
      />
    </div>
  );
}

function Editor({
  servers = [],
  publicBaseUrl,
  activity,
  onClearActivity,
  onNotify,
}: {
  servers?: ServerView[];
  publicBaseUrl: string;
  activity: ActivityEntry[];
  onClearActivity: () => void;
  onNotify: (msg: string) => void;
}) {
  const { id } = useParams();
  const initial = servers.find((server) => server.id === id || server.slug === id);
  if (id && !initial) {
    return (
      <div className="workspace-pad row">
        <Spinner size={18} tone="amber" />
        <p className="muted" style={{ margin: 0 }}>
          Loading server configuration…
        </p>
      </div>
    );
  }
  return (
    <div className="editor-view">
      <div className="view-header">
        <div>
          <div className="breadcrumb">
            <Link to="/">Upstreams</Link>
            <span>/</span>
            {initial ? (
              <>
                <Link to={`/servers/${initial.id}`}>{initial.slug}</Link>
                <span>/</span>
                <span>Edit</span>
              </>
            ) : (
              <span>New Upstream</span>
            )}
          </div>
          <h2>{initial ? `Edit ${initial.name}` : "Register Upstream MCP Server"}</h2>
        </div>
        {initial ? (
          <Link className="btn-ghost" to={`/servers/${initial.id}`}>
            Back to Detail
          </Link>
        ) : null}
      </div>
      <ServerForm
        key={initial?.id ?? "new"}
        initial={initial}
        publicBaseUrl={publicBaseUrl}
        activity={activity}
        onClearActivity={onClearActivity}
        onSaved={onNotify}
        onNotify={onNotify}
      />
    </div>
  );
}

function Detail({
  servers,
  activity,
  onClearActivity,
  meta,
  onNotify,
}: {
  servers: ServerView[];
  activity: ActivityEntry[];
  onClearActivity: () => void;
  meta: Meta;
  onNotify: (msg: string) => void;
}) {
  const { id } = useParams();
  const navigate = useNavigate();
  const { t } = useI18n();
  const [params] = useSearchParams();
  const server = servers.find((item) => item.id === id || item.slug === id);
  const [error, setError] = useState(params.get("oauthError") ?? "");
  const [probeBanner, setProbeBanner] = useState<ProbeResult | null>(null);
  const [busyAction, setBusyAction] = useState<
    "probe" | "reconnect" | "toggle" | "policy" | "authorize" | "clearAuth" | "delete" | null
  >(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const busy = busyAction !== null;
  const elapsedMs = useElapsedTimer(busy);

  const authLine = useMemo(
    () => (server ? describeAuth(server) : ""),
    [server],
  );

  useEffect(() => {
    setProbeBanner(null);
    setError(params.get("oauthError") ?? "");
  }, [id, params]);

  if (!server) {
    return (
      <div className="workspace-pad row">
        <Spinner size={18} tone="amber" />
        <p className="muted" style={{ margin: 0 }}>
          Waiting for server state…
        </p>
      </div>
    );
  }

  const scopedUrl = `${meta.publicBaseUrl}/mcp/${server.slug}`;
  const aggregatedUrl = `${meta.publicBaseUrl}/mcp`;
  const claudeCmd = `claude mcp add --transport http ${server.slug} ${scopedUrl}`;
  const agyCmd = `agy mcp add --transport http ${server.slug} ${scopedUrl}`;
  const codexCmd = `codex mcp add ${server.slug} --url ${scopedUrl}`;
  const geminiCmd = `gemini mcp add --transport http ${server.slug} ${scopedUrl}`;

  async function reconnect() {
    setBusyAction("reconnect");
    setError("");
    setProbeBanner(null);
    try {
      const updated = await api<ServerView>(
        `/api/servers/${server!.id}/reconnect`,
        { method: "POST" },
      );
      onNotify(
        `Reconnected ${updated.slug} (${updated.toolCount} tools)`,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : "Reconnect failed");
    } finally {
      setBusyAction(null);
    }
  }

  async function runProbe() {
    if (!server) return;
    setBusyAction("probe");
    setError("");
    try {
      const input: ServerInput = {
        name: server.name,
        slug: server.slug,
        url: server.url,
        transport: server.transport,
        enabled: server.enabled,
        serverId: server.id,
        auth: {
          kind: server.auth.kind,
          basicUsername: server.auth.basicUsername,
          basicRealm: server.auth.basicRealm,
          headers: server.auth.headerNames.map((n) => ({ name: n, value: "" })),
          oauth: server.auth.oauth
            ? {
                grant: server.auth.oauth.grant,
                clientId: server.auth.oauth.clientId,
                authorizeUrl: server.auth.oauth.authorizeUrl,
                tokenUrl: server.auth.oauth.tokenUrl,
                scopes: server.auth.oauth.scopes,
              }
            : null,
        },
      };
      const result = await api<ProbeResult>("/api/probe", {
        method: "POST",
        body: JSON.stringify(input),
      });
      setProbeBanner(result);
      onNotify(result.ok ? "Connection probe succeeded" : "Connection probe failed");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Probe failed");
    } finally {
      setBusyAction(null);
    }
  }

  async function toggleEnabled() {
    if (!server) return;
    setBusyAction("toggle");
    setError("");
    try {
      const input: ServerInput = {
        name: server.name,
        slug: server.slug,
        url: server.url,
        transport: server.transport,
        enabled: !server.enabled,
        auth: {
          kind: server.auth.kind,
          basicUsername: server.auth.basicUsername,
          basicRealm: server.auth.basicRealm,
          headers: server.auth.headerNames.map((n) => ({ name: n, value: "" })),
          oauth: server.auth.oauth
            ? {
                grant: server.auth.oauth.grant,
                clientId: server.auth.oauth.clientId,
                authorizeUrl: server.auth.oauth.authorizeUrl,
                tokenUrl: server.auth.oauth.tokenUrl,
                scopes: server.auth.oauth.scopes,
              }
            : null,
        },
      };
      const updated = await api<ServerView>(`/api/servers/${server.id}`, {
        method: "PUT",
        body: JSON.stringify(input),
      });
      onNotify(
        `${updated.slug} ${updated.enabled ? "enabled" : "disabled"}`,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : "Toggle failed");
    } finally {
      setBusyAction(null);
    }
  }

  async function toggleReadOnlyShield() {
    if (!server) return;
    setBusyAction("policy");
    setError("");
    const isReadOnly = server.toolPolicy?.mode === "readOnly";
    const nextMode = isReadOnly ? "allowAll" : "readOnly";
    try {
      const updated = await api<ServerView>(`/api/servers/${server.id}/policy`, {
        method: "PUT",
        body: JSON.stringify({
          mode: nextMode,
          allowedTools: server.toolPolicy?.allowedTools ?? [],
          deniedTools: server.toolPolicy?.deniedTools ?? [],
        }),
      });
      onNotify(
        updated.toolPolicy?.mode === "readOnly"
          ? `🛡️ Read-Only Shield enabled on ${updated.slug} (${updated.exposedToolCount}/${updated.toolCount} tools exposed)`
          : `Read-Only Shield disabled on ${updated.slug}`,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : "Policy update failed");
    } finally {
      setBusyAction(null);
    }
  }

  async function confirmRemove() {
    if (!server) return;
    setBusyAction("delete");
    try {
      await api(`/api/servers/${server.id}`, { method: "DELETE" });
      onNotify(`Deleted upstream ${server.slug}`);
      navigate("/");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Delete failed");
      setBusyAction(null);
      setConfirmDelete(false);
    }
  }

  async function authorize() {
    setBusyAction("authorize");
    try {
      const started = await api<{ authorizeUrl: string }>(
        `/api/servers/${server!.id}/oauth/start`,
        { method: "POST" },
      );
      window.location.href = started.authorizeUrl;
    } catch (err) {
      setError(err instanceof Error ? err.message : "OAuth start failed");
      setBusyAction(null);
    }
  }

  async function clearAuthTokens() {
    if (!server) return;
    setBusyAction("clearAuth");
    setError("");
    try {
      await api<ServerView>(`/api/servers/${server.id}/auth/clear`, {
        method: "POST",
      });
      onNotify(`Revoked & cleared stored credentials for ${server.slug}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear credentials");
    } finally {
      setBusyAction(null);
    }
  }

  return (
    <div className="detail-view">
      {/* Detail Hero Header */}
      <div className="detail-header-card">
        <div className="detail-title-group">
          <div className="breadcrumb">
            <Link to="/">Upstreams</Link>
            <span>/</span>
            <span>{server.slug}</span>
          </div>
          <div className="row" style={{ gap: 12, marginTop: 4 }}>
            <ServiceIcon slug={server.slug} name={server.name} size={30} />
            <h2>{server.name}</h2>
            <span className={`status-pill ${server.state}`}>
              {server.state === "connecting" || busyAction === "reconnect" ? (
                <Spinner size={10} tone="amber" />
              ) : (
                <i className={`lamp ${lampClass(server.state)}`} />
              )}
              {label(server.state)}
            </span>
            <span className="badge-pill">{server.transport}</span>
            <span className="tool-count-pill">
              {server.exposedToolCount !== undefined &&
              server.exposedToolCount < server.toolCount
                ? `${server.exposedToolCount}/${server.toolCount} tools exposed`
                : `${server.toolCount} tools`}
            </span>
          </div>
        </div>

        <div className="detail-actions">
          <label
            className="switch-label compact"
            title="Block all write/destructive tools from local CLI agents (/mcp and /mcp/{slug})"
          >
            <input
              type="checkbox"
              checked={server.toolPolicy?.mode === "readOnly"}
              disabled={busy}
              onChange={toggleReadOnlyShield}
            />
            <span className="switch-track" />
            <span className="row" style={{ gap: 6 }}>
              {busyAction === "policy" ? <Spinner size={12} tone="amber" /> : null}
              {t("bench.readOnlyShield")}
            </span>
          </label>

          <label className="switch-label compact" title="Enable or disable this upstream">
            <input
              type="checkbox"
              checked={server.enabled}
              disabled={busy}
              onChange={toggleEnabled}
            />
            <span className="switch-track" />
            <span className="row" style={{ gap: 6 }}>
              {busyAction === "toggle" ? <Spinner size={12} tone="amber" /> : null}
              {server.enabled ? t("common.enabled") : t("common.disabled")}
            </span>
          </label>

          <button
            type="button"
            className="btn-ghost"
            disabled={busy}
            onClick={runProbe}
          >
            {busyAction === "probe" ? (
              <>
                <Spinner size={13} tone="amber" />
                <span>{t("form.probingMs", { ms: elapsedMs })}</span>
              </>
            ) : (
              t("form.testConnection")
            )}
          </button>
          <button
            type="button"
            className="btn"
            disabled={busy}
            onClick={reconnect}
          >
            {busyAction === "reconnect" ? (
              <>
                <Spinner size={13} tone="amber" />
                <span>{t("detail.reconnectingMs", { ms: elapsedMs })}</span>
              </>
            ) : (
              `↻ ${t("detail.reconnectBtn")}`
            )}
          </button>
          {server.auth.kind === "oauth2" &&
          server.auth.oauth?.grant === "authorization_code" ? (
            <button
              type="button"
              className="btn primary"
              disabled={busy}
              onClick={authorize}
            >
              {busyAction === "authorize" ? (
                <>
                  <Spinner size={13} tone="dark" />
                  <span>Redirecting…</span>
                </>
              ) : (
                t("detail.authorizeOAuth")
              )}
            </button>
          ) : null}
          <Link className="btn-ghost" to={`/servers/${server.id}/edit`}>
            {t("detail.editConfig")}
          </Link>
          {server.auth.kind !== "none" &&
          (server.auth.bearerConfigured ||
            server.auth.oauth?.hasAccessToken ||
            server.auth.oauth?.hasRefreshToken ||
            server.auth.headerNames.length > 0 ||
            Boolean(server.auth.basicUsername)) ? (
            <button
              type="button"
              className="btn-ghost"
              disabled={busy}
              onClick={() => void clearAuthTokens()}
              title="Wipe stored tokens/keys from the encrypted vault without deleting the upstream"
            >
              {busyAction === "clearAuth" ? (
                <>
                  <Spinner size={13} tone="amber" />
                  <span>Clearing ({elapsedMs}ms)…</span>
                </>
              ) : (
                t("detail.revokeAuth")
              )}
            </button>
          ) : null}
          <button
            type="button"
            className="btn-ghost danger"
            disabled={busy}
            onClick={() => setConfirmDelete(true)}
          >
            {t("detail.deleteServer")}
          </button>
        </div>
      </div>

      {error ? (
        <div className="callout bad callout-anim">
          <div className="callout-title">Error</div>
          <p>{error}</p>
        </div>
      ) : null}

      {server.message ? (
        <div
          className={`callout callout-anim ${server.state === "error" || server.state === "authRequired" ? "bad" : "warning"}`}
        >
          <div className="callout-title">Upstream Status Message</div>
          <p>{server.message}</p>
        </div>
      ) : null}

      {probeBanner ? (
        <div className={`callout callout-anim ${probeBanner.ok ? "ok" : "bad"}`}>
          <div className="row" style={{ justifyContent: "space-between" }}>
            <div>
              <div className="callout-title">
                Live Probe: {probeBanner.ok ? "Connected" : "Failed"}
              </div>
              <p>{probeBanner.message}</p>
            </div>
            <button
              type="button"
              className="btn-link"
              onClick={() => setProbeBanner(null)}
            >
              Dismiss
            </button>
          </div>
        </div>
      ) : null}

      {/* Two-Column Info Cards */}
      <div className="detail-grid">
        <div className="info-card">
          <h3>Local Client Endpoints (Unauthenticated)</h3>
          <p className="muted" style={{ marginBottom: 12 }}>
            Use these loopback URLs in your local CLI tools. Credentials never leave the proxy container.
          </p>
          <div className="endpoint-copy-list">
            <div className="endpoint-copy-row">
              <div>
                <span className="badge-tag">Scoped Endpoint (Recommended)</span>
                <code>{scopedUrl}</code>
                <small className="muted">Exposes this server's original tool names</small>
              </div>
              <CopyButton
                text={scopedUrl}
                label="Copy URL"
                className="btn-ghost compact"
                onCopied={() => onNotify("Copied scoped MCP URL")}
              />
            </div>
            <div className="endpoint-copy-row">
              <div>
                <span className="badge-tag subtle">Claude Code CLI</span>
                <code>{claudeCmd}</code>
              </div>
              <CopyButton
                text={claudeCmd}
                label="Copy CLI"
                className="btn-ghost compact"
                onCopied={() => onNotify("Copied Claude Code CLI command")}
              />
            </div>
            <div className="endpoint-copy-row">
              <div>
                <span className="badge-tag subtle">Antigravity (agy) CLI</span>
                <code>{agyCmd}</code>
              </div>
              <CopyButton
                text={agyCmd}
                label="Copy CLI"
                className="btn-ghost compact"
                onCopied={() => onNotify("Copied AGY CLI command")}
              />
            </div>
            <div className="endpoint-copy-row">
              <div>
                <span className="badge-tag subtle">OpenAI Codex CLI</span>
                <code>{codexCmd}</code>
              </div>
              <CopyButton
                text={codexCmd}
                label="Copy CLI"
                className="btn-ghost compact"
                onCopied={() => onNotify("Copied Codex CLI command")}
              />
            </div>
            <div className="endpoint-copy-row">
              <div>
                <span className="badge-tag subtle">Gemini CLI</span>
                <code>{geminiCmd}</code>
              </div>
              <CopyButton
                text={geminiCmd}
                label="Copy CLI"
                className="btn-ghost compact"
                onCopied={() => onNotify("Copied Gemini CLI command")}
              />
            </div>
            <div className="endpoint-copy-row">
              <div>
                <span className="badge-tag subtle">Aggregated Endpoint</span>
                <code>{aggregatedUrl}</code>
                <small className="muted">
                  Prefixes tools with <code>{server.slug}__</code>
                </small>
              </div>
              <CopyButton
                text={aggregatedUrl}
                label="Copy URL"
                className="btn-ghost compact"
                onCopied={() => onNotify("Copied aggregated MCP URL")}
              />
            </div>
          </div>
        </div>

        <div className="info-card">
          <h3>Credential Vault & Upstream Telemetry</h3>
          <dl className="meta-grid">
            <dt>Remote URL</dt>
            <dd>
              <code>{server.url}</code>
            </dd>
            <dt>Vault Strategy</dt>
            <dd>
              <span className="vault-badge">{authLine}</span>
            </dd>
            <dt>Transport</dt>
            <dd>{server.transport}</dd>
            <dt>Runtime Telemetry</dt>
            <dd>
              {server.totalCalls ?? 0} calls · {server.errorCount ?? 0} errors
              {server.lastLatencyMs !== undefined && server.lastLatencyMs !== null
                ? ` · last ${server.lastLatencyMs}ms`
                : ""}
            </dd>
            <dt>Last Checked</dt>
            <dd>{server.checkedAt ? formatTime(server.checkedAt) : "Not checked yet"}</dd>
            <dt>Updated At</dt>
            <dd>{formatTime(server.updatedAt)}</dd>
          </dl>
        </div>
      </div>

      {/* Tool Workbench Section */}
      <div className="bench-section">
        <div className="bench-section-head">
          <div>
            <h2>{t("detail.workbenchTitle")}</h2>
            <p className="muted">
              {t("detail.workbenchSubtitle")}
            </p>
          </div>
        </div>
        <ToolBench
          key={server.id}
          server={server}
          initialToolName={params.get("tool") ?? undefined}
          onNotify={onNotify}
        />
      </div>

      {/* Live Terminal Gadget for this Server */}
      <LiveTerminalGadget
        entries={activity}
        filterSlug={server.slug}
        title={`mcp-terminal — /mcp/${server.slug}`}
        busy={busy}
        onClear={onClearActivity}
        onNotify={onNotify}
        maxHeight={300}
      />

      {/* Delete Confirmation Modal */}
      {confirmDelete ? (
        <div className="modal-backdrop" onClick={() => !busy && setConfirmDelete(false)}>
          <div
            className="modal-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <h3>Delete upstream "{server.name}"?</h3>
            <p className="muted">
              This will permanently remove <code>{server.slug}</code> and erase its encrypted credentials from the
              SQLite vault. Any local CLI connected to <code>/mcp/{server.slug}</code> will lose access to its{" "}
              {server.toolCount} tools.
            </p>
            <div className="modal-actions">
              <button
                type="button"
                className="btn-ghost"
                disabled={busy}
                onClick={() => setConfirmDelete(false)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn danger-fill"
                disabled={busy}
                onClick={confirmRemove}
              >
                {busyAction === "delete" ? (
                  <>
                    <Spinner size={14} tone="current" />
                    <span>Deleting…</span>
                  </>
                ) : (
                  `Delete ${server.slug}`
                )}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function useLive(enabled: boolean) {
  const [servers, setServers] = useState<ServerView[]>([]);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [connected, setConnected] = useState(false);

  const clearActivity = useCallback(() => {
    setActivity([]);
    void api("/api/activity", { method: "DELETE" }).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!enabled) return;
    let closed = false;
    let socket: WebSocket | null = null;
    let timer = 0;
    const apply = (message: {
      type?: string;
      servers?: ServerView[];
      activity?: ActivityEntry[];
      server?: ServerView;
      entry?: ActivityEntry;
      id?: string;
    }) => {
      if (message.type === "snapshot") {
        if (message.servers) setServers(message.servers);
        if (message.activity) setActivity(message.activity);
      }
      if (message.type === "upsert" && message.server) {
        const server = message.server;
        setServers((prev) => {
          const next = prev.filter((item) => item.id !== server.id);
          next.push(server);
          return next.sort(
            (a, b) =>
              a.createdAt.localeCompare(b.createdAt) ||
              a.name.localeCompare(b.name),
          );
        });
      }
      if (message.type === "remove" && message.id) {
        const id = message.id;
        setServers((prev) => prev.filter((item) => item.id !== id));
      }
      if (message.type === "activity" && message.entry) {
        const entry = message.entry;
        setActivity((prev) => {
          const filtered = prev.filter((item) => item.seq !== entry.seq);
          filtered.push(entry);
          if (filtered.length > 200) {
            return filtered.slice(filtered.length - 200);
          }
          return filtered;
        });
      }
    };
    const connect = () => {
      socket = new WebSocket(wsUrl());
      socket.onopen = () => setConnected(true);
      socket.onclose = () => {
        setConnected(false);
        if (!closed) timer = window.setTimeout(connect, 1500);
      };
      socket.onmessage = (event) => {
        try {
          apply(JSON.parse(event.data));
        } catch {
          /* ignore malformed frames */
        }
      };
    };
    api<ServerView[]>("/api/servers")
      .then(setServers)
      .catch(() => undefined);
    connect();
    return () => {
      closed = true;
      window.clearTimeout(timer);
      socket?.close();
    };
  }, [enabled]);
  return { servers, activity, connected, clearActivity };
}

function describeAuth(server: ServerView) {
  const auth = server.auth;
  if (auth.kind === "bearer") {
    return auth.bearerConfigured
      ? "Bearer token sealed in vault"
      : "Bearer token missing";
  }
  if (auth.kind === "basic") {
    return `Basic (${auth.basicUsername ?? "user"})${auth.basicRealm ? ` · realm "${auth.basicRealm}"` : ""}`;
  }
  if (auth.kind === "headers") {
    return auth.headerNames.length
      ? `Headers: ${auth.headerNames.join(", ")}`
      : "Custom headers";
  }
  if (auth.kind === "oauth2" && auth.oauth) {
    let expiry = "";
    if (auth.oauth.expiresAt) {
      const diffSecs = Math.round(auth.oauth.expiresAt - Date.now() / 1000);
      if (diffSecs <= 0) {
        expiry = auth.oauth.hasRefreshToken
          ? " · expired (auto-refresh ready)"
          : " · token expired";
      } else if (diffSecs < 3600) {
        expiry = ` · expires in ${Math.max(1, Math.round(diffSecs / 60))}m`;
      } else {
        expiry = ` · expires in ${Math.round(diffSecs / 3600)}h`;
      }
    }
    return `OAuth2 (${auth.oauth.grant})${auth.oauth.hasRefreshToken ? " · refresh token stored" : ""}${expiry}`;
  }
  return "No credentials (Open)";
}

function authBadgeLabel(kind: string) {
  switch (kind) {
    case "oauth2":
      return "OAuth2";
    case "bearer":
      return "Bearer";
    case "basic":
      return "Basic";
    case "headers":
      return "Headers";
    default:
      return "Open";
  }
}

function lampClass(state: ConnState) {
  if (state === "connected") return "on";
  if (state === "error" || state === "authRequired") return "bad";
  if (state === "connecting" || state === "idle") return "wait";
  return "";
}

function label(state: ConnState) {
  if (state === "authRequired") return "auth required";
  return state;
}

function formatTime(iso: string) {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}
