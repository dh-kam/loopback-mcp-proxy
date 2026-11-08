import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import catppuccinLatte from "./themes/catppuccin-latte.json";
import catppuccinMocha from "./themes/catppuccin-mocha.json";
import daylightPaper from "./themes/daylight-paper.json";
import githubLight from "./themes/github-light.json";
import midnightCyan from "./themes/midnight-cyan.json";
import nordicFrost from "./themes/nordic-frost.json";
import obsidianAmber from "./themes/obsidian-amber.json";
import rosePineDawn from "./themes/rose-pine-dawn.json";
import solarizedLight from "./themes/solarized-light.json";

export interface ThemeManifest {
  id: string;
  name: string;
  author?: string;
  version?: string;
  appearance: "dark" | "light";
  description?: string;
  preview: [string, string, string];
  tokens: Record<string, string>;
  customCss?: string;
  isPlugin?: boolean;
}

export const BUILTIN_THEMES: ThemeManifest[] = [
  obsidianAmber as unknown as ThemeManifest,
  midnightCyan as unknown as ThemeManifest,
  catppuccinMocha as unknown as ThemeManifest,
  nordicFrost as unknown as ThemeManifest,
  daylightPaper as unknown as ThemeManifest,
  githubLight as unknown as ThemeManifest,
  catppuccinLatte as unknown as ThemeManifest,
  solarizedLight as unknown as ThemeManifest,
  rosePineDawn as unknown as ThemeManifest,
];

const ACTIVE_THEME_STORAGE_KEY = "loopback_ui_theme";
const PLUGIN_THEMES_STORAGE_KEY = "loopback_theme_plugins";
const PLUGIN_STYLE_TAG_ID = "loopback-theme-plugin-css";

const SAMPLE_PLUGIN_TEMPLATE: ThemeManifest = {
  id: "tokyo-night-plugin",
  name: "Tokyo Night (Plugin)",
  author: "community",
  version: "1.0.0",
  appearance: "dark",
  description: "Custom plugin theme with neon indigo and emerald highlights.",
  preview: ["#1a1b26", "#7aa2f7", "#9ece6a"],
  tokens: {
    "--bg-0": "#16161e",
    "--bg-1": "#1a1b26",
    "--bg-2": "#24283b",
    "--bg-3": "#2f3549",
    "--bg-elevated": "rgba(26, 27, 38, 0.9)",
    "--bg-topbar": "rgba(22, 22, 30, 0.86)",
    "--bg-sidebar": "rgba(26, 27, 38, 0.84)",
    "--bg-code": "#101014",
    "--ink": "#c0caf5",
    "--ink-secondary": "#a9b1d6",
    "--muted": "#68729c",
    "--line": "rgba(122, 162, 247, 0.14)",
    "--line-strong": "rgba(122, 162, 247, 0.25)",
    "--amber": "#7aa2f7",
    "--amber-soft": "rgba(122, 162, 247, 0.15)",
    "--amber-border": "rgba(122, 162, 247, 0.45)",
    "--green": "#9ece6a",
    "--green-soft": "rgba(158, 206, 106, 0.14)",
    "--green-border": "rgba(158, 206, 106, 0.4)",
    "--red": "#f7768e",
    "--red-soft": "rgba(247, 118, 142, 0.15)",
    "--red-border": "rgba(247, 118, 142, 0.45)",
    "--blue": "#bb9af7",
    "--shadow-card": "0 12px 32px rgba(10, 10, 15, 0.55)",
  },
  customCss: "",
};

const SAMPLE_LIGHT_PLUGIN_TEMPLATE: ThemeManifest = {
  id: "botanical-sage-plugin",
  name: "Botanical Sage (Plugin)",
  author: "community",
  version: "1.0.0",
  appearance: "light",
  description: "Custom light plugin theme with soft linen and botanical sage accents.",
  preview: ["#f4f6f2", "#2f6f4e", "#b45309"],
  tokens: {
    "--bg-0": "#f4f6f2",
    "--bg-1": "#ffffff",
    "--bg-2": "#e7ece3",
    "--bg-3": "#d5dfd0",
    "--bg-elevated": "rgba(255, 255, 255, 0.95)",
    "--bg-topbar": "rgba(244, 246, 242, 0.92)",
    "--bg-sidebar": "rgba(235, 240, 232, 0.94)",
    "--bg-code": "#e9eee6",
    "--ink": "#1c2821",
    "--ink-secondary": "#3b4d43",
    "--muted": "#64786c",
    "--line": "rgba(28, 40, 33, 0.11)",
    "--line-strong": "rgba(28, 40, 33, 0.19)",
    "--amber": "#2f6f4e",
    "--amber-soft": "rgba(47, 111, 78, 0.11)",
    "--amber-border": "rgba(47, 111, 78, 0.35)",
    "--green": "#15803d",
    "--green-soft": "rgba(21, 128, 61, 0.11)",
    "--green-border": "rgba(21, 128, 61, 0.34)",
    "--red": "#c2410c",
    "--red-soft": "rgba(194, 65, 12, 0.1)",
    "--red-border": "rgba(194, 65, 12, 0.34)",
    "--blue": "#0369a1",
    "--shadow-card": "0 8px 24px rgba(28, 40, 33, 0.08)",
  },
  customCss: "",
};

function loadStoredPlugins(): ThemeManifest[] {
  try {
    const raw = window.localStorage.getItem(PLUGIN_THEMES_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((item) => item && typeof item.id === "string" && typeof item.name === "string" && item.tokens)
      .map((item) => ({ ...item, isPlugin: true }));
  } catch {
    return [];
  }
}

function saveStoredPlugins(plugins: ThemeManifest[]) {
  try {
    window.localStorage.setItem(PLUGIN_THEMES_STORAGE_KEY, JSON.stringify(plugins));
  } catch {
    // ignore quota errors
  }
}

export function validateThemeManifest(raw: unknown): ThemeManifest {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error("Theme plugin manifest must be a JSON object.");
  }
  const obj = raw as Record<string, unknown>;
  const id = String(obj.id ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-_]/g, "-");
  const name = String(obj.name ?? "").trim();
  if (!id || id.length < 2) {
    throw new Error('Theme plugin must include a valid "id" (e.g. "my-theme").');
  }
  if (!name) {
    throw new Error('Theme plugin must include a display "name".');
  }
  const appearance = obj.appearance === "light" ? "light" : "dark";
  const tokensRaw = obj.tokens;
  if (!tokensRaw || typeof tokensRaw !== "object" || Array.isArray(tokensRaw)) {
    throw new Error('Theme plugin must include a "tokens" object mapping CSS variables (e.g. "--bg-0", "--amber").');
  }
  const tokens: Record<string, string> = {};
  for (const [k, v] of Object.entries(tokensRaw as Record<string, unknown>)) {
    if (k.startsWith("--") && typeof v === "string" && v.trim()) {
      tokens[k] = v.trim();
    }
  }
  if (Object.keys(tokens).length === 0) {
    throw new Error('Theme "tokens" must contain at least one "--*" CSS custom property.');
  }
  const previewArr = Array.isArray(obj.preview) ? obj.preview.map(String) : [];
  const preview: [string, string, string] = [
    previewArr[0] || tokens["--bg-0"] || "#0b0a08",
    previewArr[1] || tokens["--amber"] || "#ffb000",
    previewArr[2] || tokens["--green"] || "#4ade80",
  ];
  return {
    id,
    name,
    author: typeof obj.author === "string" ? obj.author : "custom",
    version: typeof obj.version === "string" ? obj.version : "1.0.0",
    appearance,
    description: typeof obj.description === "string" ? obj.description : "",
    preview,
    tokens,
    customCss: typeof obj.customCss === "string" ? obj.customCss : "",
    isPlugin: true,
  };
}

interface ThemeContextValue {
  themeId: string;
  activeTheme: ThemeManifest;
  themes: ThemeManifest[];
  pluginThemes: ThemeManifest[];
  setThemeId: (id: string) => void;
  installPluginTheme: (manifest: ThemeManifest) => void;
  removePluginTheme: (id: string) => void;
  studioOpen: boolean;
  setStudioOpen: (open: boolean) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [pluginThemes, setPluginThemes] = useState<ThemeManifest[]>(() =>
    loadStoredPlugins(),
  );
  const [themeId, setThemeIdState] = useState<string>(() => {
    try {
      const params = new URLSearchParams(window.location.search);
      const fromUrl = params.get("theme");
      if (fromUrl) {
        window.localStorage.setItem(ACTIVE_THEME_STORAGE_KEY, fromUrl);
        return fromUrl;
      }
      return window.localStorage.getItem(ACTIVE_THEME_STORAGE_KEY) || "obsidian-amber";
    } catch {
      return "obsidian-amber";
    }
  });
  const [studioOpen, setStudioOpen] = useState(false);

  const themes = useMemo(
    () => [...BUILTIN_THEMES, ...pluginThemes],
    [pluginThemes],
  );

  const activeTheme = useMemo(
    () => themes.find((t) => t.id === themeId) ?? BUILTIN_THEMES[0],
    [themes, themeId],
  );

  const setThemeId = useCallback((nextId: string) => {
    setThemeIdState(nextId);
    try {
      window.localStorage.setItem(ACTIVE_THEME_STORAGE_KEY, nextId);
    } catch {
      // ignore
    }
  }, []);

  const installPluginTheme = useCallback(
    (manifest: ThemeManifest) => {
      const normalized: ThemeManifest = { ...manifest, isPlugin: true };
      setPluginThemes((prev) => {
        const filtered = prev.filter(
          (item) =>
            item.id !== normalized.id &&
            !BUILTIN_THEMES.some((b) => b.id === normalized.id),
        );
        const safeId = BUILTIN_THEMES.some((b) => b.id === normalized.id)
          ? `${normalized.id}-plugin`
          : normalized.id;
        const entry = { ...normalized, id: safeId };
        const next = [...filtered.filter((p) => p.id !== safeId), entry];
        saveStoredPlugins(next);
        setThemeId(safeId);
        return next;
      });
    },
    [setThemeId],
  );

  const removePluginTheme = useCallback(
    (targetId: string) => {
      setPluginThemes((prev) => {
        const next = prev.filter((p) => p.id !== targetId);
        saveStoredPlugins(next);
        return next;
      });
      if (themeId === targetId) {
        setThemeId(BUILTIN_THEMES[0].id);
      }
    },
    [themeId, setThemeId],
  );

  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute("data-theme", activeTheme.id);
    root.setAttribute("data-appearance", activeTheme.appearance);
    root.style.colorScheme = activeTheme.appearance;

    // Reset baseline tokens from default theme (light or dark) first so partial plugins inherit cleanly
    const baseTokens =
      activeTheme.appearance === "light"
        ? (daylightPaper as unknown as ThemeManifest).tokens
        : BUILTIN_THEMES[0].tokens;
    for (const [k, v] of Object.entries(baseTokens)) {
      root.style.setProperty(k, v);
    }
    for (const [k, v] of Object.entries(activeTheme.tokens)) {
      root.style.setProperty(k, v);
    }

    let styleTag = document.getElementById(PLUGIN_STYLE_TAG_ID) as HTMLStyleElement | null;
    if (activeTheme.customCss && activeTheme.customCss.trim()) {
      if (!styleTag) {
        styleTag = document.createElement("style");
        styleTag.id = PLUGIN_STYLE_TAG_ID;
        document.head.appendChild(styleTag);
      }
      styleTag.textContent = activeTheme.customCss;
    } else if (styleTag) {
      styleTag.textContent = "";
    }
  }, [activeTheme]);

  const value = useMemo<ThemeContextValue>(
    () => ({
      themeId: activeTheme.id,
      activeTheme,
      themes,
      pluginThemes,
      setThemeId,
      installPluginTheme,
      removePluginTheme,
      studioOpen,
      setStudioOpen,
    }),
    [
      activeTheme,
      themes,
      pluginThemes,
      setThemeId,
      installPluginTheme,
      removePluginTheme,
      studioOpen,
    ],
  );

  return (
    <ThemeContext.Provider value={value}>
      {children}
      {studioOpen ? <ThemePluginStudioModal /> : null}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used inside <ThemeProvider>");
  }
  return ctx;
}

export function ThemeSwitcher({
  onSwitched,
}: {
  onSwitched?: (theme: ThemeManifest) => void;
}) {
  const { activeTheme, themes, setThemeId, setStudioOpen } = useTheme();
  const [open, setOpen] = useState(false);
  const [modeFilter, setModeFilter] = useState<"all" | "light" | "dark">("all");
  const wrapRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", onDocClick);
    return () => window.removeEventListener("mousedown", onDocClick);
  }, [open]);

  const lightCount = themes.filter((t) => t.appearance === "light").length;
  const darkCount = themes.filter((t) => t.appearance === "dark").length;
  const filteredThemes = themes.filter((t) =>
    modeFilter === "all" ? true : t.appearance === modeFilter,
  );

  return (
    <div className="theme-switcher-wrap" ref={wrapRef}>
      <button
        type="button"
        className="theme-switcher-trigger"
        onClick={() => setOpen((p) => !p)}
        title={`Theme: ${activeTheme.name} (click to switch or install plugin theme)`}
      >
        <span className="theme-swatch-Cluster" aria-hidden="true">
          <i style={{ background: activeTheme.preview[0] }} />
          <i style={{ background: activeTheme.preview[1] }} />
          <i style={{ background: activeTheme.preview[2] }} />
        </span>
        <span className="theme-trigger-name">{activeTheme.name}</span>
        <span className="theme-trigger-caret">▾</span>
      </button>

      {open ? (
        <div className="theme-popover callout-anim">
          <div className="theme-popover-head">
            <span>UI THEME &amp; PLUGINS</span>
            <button
              type="button"
              className="theme-studio-link"
              onClick={() => {
                setOpen(false);
                setStudioOpen(true);
              }}
            >
              + Plugin Studio
            </button>
          </div>
          <div className="theme-filter-pills">
            <button
              type="button"
              className={modeFilter === "all" ? "on" : ""}
              onClick={() => setModeFilter("all")}
            >
              All ({themes.length})
            </button>
            <button
              type="button"
              className={modeFilter === "light" ? "on" : ""}
              onClick={() => setModeFilter("light")}
            >
              ☀ Light ({lightCount})
            </button>
            <button
              type="button"
              className={modeFilter === "dark" ? "on" : ""}
              onClick={() => setModeFilter("dark")}
            >
              ☾ Dark ({darkCount})
            </button>
          </div>
          <div className="theme-popover-list">
            {filteredThemes.map((item) => {
              const isCurrent = item.id === activeTheme.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  className={`theme-option-item ${isCurrent ? "active" : ""}`}
                  onClick={() => {
                    setThemeId(item.id);
                    onSwitched?.(item);
                    setOpen(false);
                  }}
                >
                  <span className="theme-swatch-Cluster">
                    <i style={{ background: item.preview[0] }} />
                    <i style={{ background: item.preview[1] }} />
                    <i style={{ background: item.preview[2] }} />
                  </span>
                  <div className="theme-option-meta">
                    <div className="row" style={{ gap: 6 }}>
                      <strong>{item.name}</strong>
                      <span
                        className={`theme-mode-tag ${item.appearance === "light" ? "light" : "dark"}`}
                      >
                        {item.appearance === "light" ? "☀ LIGHT" : "☾ DARK"}
                      </span>
                      {item.isPlugin ? (
                        <span className="theme-plugin-tag">PLUGIN</span>
                      ) : null}
                    </div>
                    {item.description ? (
                      <small className="muted">{item.description}</small>
                    ) : null}
                  </div>
                  {isCurrent ? <span className="theme-check">✓</span> : null}
                </button>
              );
            })}
          </div>
          <div className="theme-popover-foot">
            <button
              type="button"
              className="btn compact"
              style={{ width: "100%", justifyContent: "center" }}
              onClick={() => {
                setOpen(false);
                setStudioOpen(true);
              }}
            >
              🎨 Install / Create Theme Plugin (.json)
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

function ThemePluginStudioModal() {
  const {
    activeTheme,
    themes,
    setThemeId,
    installPluginTheme,
    removePluginTheme,
    setStudioOpen,
  } = useTheme();

  const [jsonInput, setJsonInput] = useState<string>(() =>
    JSON.stringify(SAMPLE_PLUGIN_TEMPLATE, null, 2),
  );
  const [error, setError] = useState<string>("");
  const [statusMsg, setStatusMsg] = useState<string>("");
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  function handleInstallFromEditor() {
    setError("");
    setStatusMsg("");
    try {
      const parsed = JSON.parse(jsonInput);
      const manifest = validateThemeManifest(parsed);
      installPluginTheme(manifest);
      setStatusMsg(`✓ Installed and activated plugin theme "${manifest.name}"`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Invalid theme plugin JSON");
    }
  }

  function handleExportTheme(target: ThemeManifest) {
    const blob = new Blob([JSON.stringify(target, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${target.id}.theme.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div className="modal-backdrop" onClick={() => setStudioOpen(false)}>
      <div
        className="modal-dialog theme-studio-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="row"
          style={{ justifyContent: "space-between", marginBottom: 10 }}
        >
          <div>
            <h3 style={{ margin: 0 }}>🎨 Theme &amp; Plugin Studio</h3>
            <small className="muted">
              Switch built-in themes or install custom <code>.theme.json</code>{" "}
              plugin manifests at runtime.
            </small>
          </div>
          <button
            type="button"
            className="btn-ghost compact"
            onClick={() => setStudioOpen(false)}
          >
            ✕ Close
          </button>
        </div>

        {/* Installed & Built-in Theme Gallery */}
        <div className="theme-gallery-grid">
          {themes.map((t) => {
            const active = t.id === activeTheme.id;
            return (
              <div
                key={t.id}
                className={`theme-gallery-card ${active ? "active" : ""}`}
                onClick={() => setThemeId(t.id)}
              >
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <span className="theme-swatch-Cluster">
                    <i style={{ background: t.preview[0] }} />
                    <i style={{ background: t.preview[1] }} />
                    <i style={{ background: t.preview[2] }} />
                  </span>
                  <div className="row" style={{ gap: 4 }}>
                    <span className="theme-mode-tag">
                      {t.appearance.toUpperCase()}
                    </span>
                    {t.isPlugin ? (
                      <span className="theme-plugin-tag">PLUGIN</span>
                    ) : null}
                  </div>
                </div>
                <strong style={{ fontSize: 13 }}>{t.name}</strong>
                <small className="muted" style={{ fontSize: 11 }}>
                  {t.description || `ID: ${t.id}`}
                </small>
                <div
                  className="row"
                  style={{ gap: 6, marginTop: 4 }}
                  onClick={(e) => e.stopPropagation()}
                >
                  <button
                    type="button"
                    className="btn-link"
                    style={{ fontSize: 11 }}
                    onClick={() => {
                      const clone: ThemeManifest = {
                        ...t,
                        id: `${t.id}-custom`,
                        name: `${t.name} (Custom)`,
                        isPlugin: true,
                      };
                      setJsonInput(JSON.stringify(clone, null, 2));
                      setStatusMsg(`Loaded "${t.name}" into plugin editor`);
                    }}
                  >
                    Clone to Editor
                  </button>
                  <button
                    type="button"
                    className="btn-link"
                    style={{ fontSize: 11 }}
                    onClick={() => handleExportTheme(t)}
                  >
                    ⬇ Export .json
                  </button>
                  {t.isPlugin ? (
                    <button
                      type="button"
                      className="btn-link"
                      style={{ fontSize: 11, color: "var(--red)" }}
                      onClick={() => removePluginTheme(t.id)}
                    >
                      Delete
                    </button>
                  ) : null}
                </div>
              </div>
            );
          })}
        </div>

        {/* Plugin JSON Editor & File Importer */}
        <div className="theme-plugin-editor-box">
          <div
            className="row"
            style={{ justifyContent: "space-between", marginBottom: 6 }}
          >
            <strong style={{ fontSize: 12 }}>
              Install or Author Theme Plugin Manifest (<code>.theme.json</code>)
            </strong>
            <div className="row" style={{ gap: 8 }}>
              <input
                ref={fileInputRef}
                type="file"
                accept=".json,application/json"
                style={{ display: "none" }}
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (!file) return;
                  const reader = new FileReader();
                  reader.onload = () => {
                    const content = String(reader.result ?? "");
                    setJsonInput(content);
                    try {
                      const manifest = validateThemeManifest(JSON.parse(content));
                      installPluginTheme(manifest);
                      setStatusMsg(
                        `✓ Imported & activated "${manifest.name}" from ${file.name}`,
                      );
                      setError("");
                    } catch (err) {
                      setError(
                        err instanceof Error
                          ? err.message
                          : "Failed to parse theme file",
                      );
                    }
                  };
                  reader.readAsText(file);
                }}
              />
              <button
                type="button"
                className="btn compact"
                onClick={() => fileInputRef.current?.click()}
              >
                📂 Upload .theme.json
              </button>
              <button
                type="button"
                className="btn compact ghost"
                onClick={() => {
                  setJsonInput(JSON.stringify(SAMPLE_LIGHT_PLUGIN_TEMPLATE, null, 2));
                  setError("");
                }}
              >
                ☀ Light Template
              </button>
              <button
                type="button"
                className="btn compact ghost"
                onClick={() => {
                  setJsonInput(JSON.stringify(SAMPLE_PLUGIN_TEMPLATE, null, 2));
                  setError("");
                }}
              >
                ☾ Dark Template
              </button>
            </div>
          </div>

          <textarea
            className="code-editor"
            rows={10}
            spellCheck={false}
            value={jsonInput}
            onChange={(e) => setJsonInput(e.target.value)}
          />

          {error ? (
            <div className="callout bad callout-anim" style={{ marginTop: 8 }}>
              {error}
            </div>
          ) : null}
          {statusMsg ? (
            <div className="callout ok callout-anim" style={{ marginTop: 8 }}>
              {statusMsg}
            </div>
          ) : null}

          <div
            className="row"
            style={{ justifyContent: "space-between", marginTop: 10 }}
          >
            <small className="muted">
              Supports all <code>--bg-*</code>, <code>--ink</code>,{" "}
              <code>--amber</code>, <code>--green</code>, <code>--red</code>{" "}
              tokens + optional <code>customCss</code>.
            </small>
            <button
              type="button"
              className="btn primary"
              onClick={handleInstallFromEditor}
            >
              ⚡ Install &amp; Activate Plugin Theme
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
