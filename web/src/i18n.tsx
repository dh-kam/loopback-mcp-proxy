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
import en from "./locales/en.json";
import es from "./locales/es.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";

export type TranslationKey = keyof typeof en;
export type TranslationDictionary = Partial<Record<TranslationKey, string>>;

export interface LocaleMeta {
  code: string;
  label: string;
  nativeLabel: string;
  shortBadge: string;
  messages: TranslationDictionary;
}

export const SUPPORTED_LOCALES: LocaleMeta[] = [
  {
    code: "en",
    label: "English",
    nativeLabel: "English",
    shortBadge: "EN",
    messages: en,
  },
  {
    code: "ko",
    label: "Korean",
    nativeLabel: "한국어",
    shortBadge: "KO",
    messages: ko,
  },
  {
    code: "ja",
    label: "Japanese",
    nativeLabel: "日本語",
    shortBadge: "JA",
    messages: ja,
  },
  {
    code: "es",
    label: "Spanish",
    nativeLabel: "Español",
    shortBadge: "ES",
    messages: es,
  },
];

const STORAGE_KEY = "loopback_ui_lang";

function detectInitialLocale(): string {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && SUPPORTED_LOCALES.some((l) => l.code === saved)) {
      return saved;
    }
  } catch {
    // ignore storage access issues
  }
  if (typeof navigator !== "undefined" && navigator.language) {
    const prefix = navigator.language.toLowerCase().split("-")[0];
    const match = SUPPORTED_LOCALES.find((l) => l.code === prefix);
    if (match) return match.code;
  }
  return "en";
}

export interface I18nContextValue {
  lang: string;
  setLang: (code: string) => void;
  t: (key: TranslationKey, vars?: Record<string, string | number>) => string;
  locales: LocaleMeta[];
  currentLocale: LocaleMeta;
}

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<string>(() => detectInitialLocale());

  const setLang = useCallback((code: string) => {
    if (!SUPPORTED_LOCALES.some((l) => l.code === code)) return;
    setLangState(code);
    try {
      localStorage.setItem(STORAGE_KEY, code);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.lang = lang;
    }
  }, [lang]);

  const currentLocale = useMemo(
    () =>
      SUPPORTED_LOCALES.find((l) => l.code === lang) ?? SUPPORTED_LOCALES[0],
    [lang],
  );

  const t = useCallback(
    (key: TranslationKey, vars?: Record<string, string | number>): string => {
      const raw = currentLocale.messages[key] ?? en[key] ?? key;
      if (!vars) return raw;
      return raw.replace(/\{([a-zA-Z0-9_]+)\}/g, (_match, k: string) =>
        vars[k] !== undefined ? String(vars[k]) : `{${k}}`,
      );
    },
    [currentLocale],
  );

  const value = useMemo<I18nContextValue>(
    () => ({
      lang,
      setLang,
      t,
      locales: SUPPORTED_LOCALES,
      currentLocale,
    }),
    [lang, setLang, t, currentLocale],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    // Fallback if rendered outside provider
    return {
      lang: "en",
      setLang: () => {},
      t: (key, vars) => {
        const raw = en[key] ?? key;
        if (!vars) return raw;
        return raw.replace(/\{([a-zA-Z0-9_]+)\}/g, (_m, k: string) =>
          vars[k] !== undefined ? String(vars[k]) : `{${k}}`,
        );
      },
      locales: SUPPORTED_LOCALES,
      currentLocale: SUPPORTED_LOCALES[0],
    };
  }
  return ctx;
}

export function LanguageSwitcher({
  onSwitched,
}: {
  onSwitched?: (locale: LocaleMeta) => void;
}) {
  const { lang, setLang, locales, currentLocale, t } = useI18n();
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", onDocClick);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("mousedown", onDocClick);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div className="lang-switcher-wrap" ref={wrapRef}>
      <button
        type="button"
        className="lang-switcher-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        title={t("topbar.langTitle")}
        onClick={() => setOpen((prev) => !prev)}
      >
        <svg
          className="lang-globe-svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          aria-hidden="true"
        >
          <circle
            cx="12"
            cy="12"
            r="9"
            stroke="currentColor"
            strokeWidth="1.8"
          />
          <path
            d="M3.6 9h16.8M3.6 15h16.8M12 3a14.5 14.5 0 0 1 0 18M12 3a14.5 14.5 0 0 0 0 18"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
          />
        </svg>
        <span className="lang-trigger-label">{currentLocale.nativeLabel}</span>
        <span className="lang-trigger-caret">▾</span>
      </button>

      {open ? (
        <div
          className="lang-popover callout-anim"
          role="listbox"
          aria-label={t("topbar.langTitle")}
        >
          <div className="lang-popover-head">
            <span>{t("topbar.langTitle")}</span>
          </div>
          <div className="lang-popover-list">
            {locales.map((loc) => {
              const active = loc.code === lang;
              return (
                <button
                  key={loc.code}
                  type="button"
                  role="option"
                  aria-selected={active}
                  className={`lang-option-item ${active ? "active" : ""}`}
                  onClick={() => {
                    if (loc.code !== lang) {
                      setLang(loc.code);
                      onSwitched?.(loc);
                    }
                    setOpen(false);
                  }}
                >
                  <span className="lang-code-badge">{loc.shortBadge}</span>
                  <div className="lang-option-meta">
                    <strong>{loc.nativeLabel}</strong>
                    <small className="muted">{loc.label}</small>
                  </div>
                  {active ? <span className="lang-check">✓</span> : null}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}
