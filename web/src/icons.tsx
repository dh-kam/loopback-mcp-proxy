import type { ReactNode } from "react";

/**
 * Resolves a canonical service key from a slug, name, or install command.
 */
export function resolveServiceKey(raw: string): string {
  const s = raw.toLowerCase();
  if (s.includes("notion")) return "notion";
  if (s.includes("linear")) return "linear";
  if (s.includes("github")) return "github";
  if (s.includes("stripe")) return "stripe";
  if (s.includes("sentry")) return "sentry";
  if (s.includes("supabase")) return "supabase";
  if (s.includes("vercel")) return "vercel";
  if (s.includes("figma")) return "figma";
  if (s.includes("atlassian") || s.includes("jira") || s.includes("confluence")) return "atlassian";
  if (s.includes("trello")) return "trello";
  if (s.includes("cloudflare")) return "cloudflare";
  if (s.includes("context7")) return "context7";
  if (s.includes("deepwiki")) return "deepwiki";
  if (s.includes("gitlab")) return "gitlab";
  if (s.includes("huggingface") || s.includes("hugging face")) return "huggingface";
  if (s.includes("postman")) return "postman";
  if (s.includes("semgrep")) return "semgrep";
  if (s.includes("neon")) return "neon";
  if (s.includes("mongodb")) return "mongodb";
  if (s.includes("prisma")) return "prisma";
  if (s.includes("posthog")) return "posthog";
  if (s.includes("pagerduty")) return "pagerduty";
  if (s.includes("virustotal")) return "virustotal";
  if (s.includes("asana")) return "asana";
  if (s.includes("monday")) return "monday";
  if (s.includes("intercom")) return "intercom";
  if (s.includes("hubspot")) return "hubspot";
  if (s.includes("dropbox")) return "dropbox";
  if (s.includes("box")) return "box";
  if (s.includes("pandadoc")) return "pandadoc";
  if (s.includes("netlify")) return "netlify";
  if (s.includes("railway")) return "railway";
  if (s.includes("paypal")) return "paypal";
  if (s.includes("square")) return "square";
  if (s.includes("canva")) return "canva";
  if (s.includes("zapier")) return "zapier";
  if (s.includes("slack")) return "slack";
  return s.replace(/[^a-z0-9]/g, "") || "mcp";
}

interface BrandPalette {
  bg: string;
  border: string;
  fg: string;
  glyph: ReactNode;
}

function getServicePalette(key: string, rawLabel: string): BrandPalette {
  switch (key) {
    case "notion":
      return {
        bg: "linear-gradient(135deg, #262626 0%, #141414 100%)",
        border: "rgba(255, 255, 255, 0.24)",
        fg: "#ffffff",
        glyph: (
          <path
            d="M6.5 5.5h3.2l4.8 6.8V5.5H17v13h-2.9L9 11.2v7.3H6.5v-13Z"
            fill="currentColor"
          />
        ),
      };
    case "linear":
      return {
        bg: "linear-gradient(135deg, rgba(94, 106, 210, 0.28) 0%, rgba(46, 52, 116, 0.35) 100%)",
        border: "rgba(116, 128, 242, 0.45)",
        fg: "#8b98ff",
        glyph: (
          <>
            <circle cx="12" cy="12" r="7.5" stroke="currentColor" strokeWidth="1.8" />
            <path
              d="M6.8 14.8l8-8M5.8 11.4l5.6-5.6M9.5 17.6l8.1-8.1"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
            />
          </>
        ),
      };
    case "github":
      return {
        bg: "linear-gradient(135deg, #24292f 0%, #161b22 100%)",
        border: "rgba(110, 118, 129, 0.45)",
        fg: "#f0f6fc",
        glyph: (
          <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M12 4.2a7.8 7.8 0 0 0-2.47 15.2c.39.07.53-.17.53-.38v-1.35c-2.17.47-2.63-.92-2.63-.92-.35-.9-.87-1.14-.87-1.14-.71-.49.05-.48.05-.48.78.06 1.2.8 1.2.8.7 1.2 1.83.85 2.28.65.07-.5.27-.85.5-1.05-1.73-.2-3.55-.86-3.55-3.85 0-.85.3-1.55.8-2.1-.08-.2-.35-.99.08-2.06 0 0 .65-.21 2.15.8a7.4 7.4 0 0 1 3.9 0c1.5-1.01 2.15-.8 2.15-.8.43 1.07.16 1.86.08 2.06.5.55.8 1.25.8 2.1 0 3-1.83 3.65-3.57 3.85.28.24.53.72.53 1.46v2.16c0 .21.14.46.54.38A7.8 7.8 0 0 0 12 4.2Z"
            fill="currentColor"
          />
        ),
      };
    case "stripe":
      return {
        bg: "linear-gradient(135deg, rgba(99, 91, 255, 0.28) 0%, rgba(67, 56, 202, 0.32) 100%)",
        border: "rgba(122, 115, 255, 0.48)",
        fg: "#9892ff",
        glyph: (
          <path
            d="M11.3 9.2c0-.7.6-1 1.5-1 1.3 0 2.9.4 4.2 1.1V6.1c-1.4-.6-2.8-.8-4.2-.8-3.4 0-5.7 1.8-5.7 4.7 0 4.5 6.2 3.8 6.2 5.8 0 .8-.7 1.1-1.7 1.1-1.5 0-3.4-.6-4.8-1.4v3.3c1.6.7 3.2 1 4.8 1 3.5 0 5.9-1.7 5.9-4.7 0-4.9-6.2-4-6.2-5.9Z"
            fill="currentColor"
          />
        ),
      };
    case "sentry":
      return {
        bg: "linear-gradient(135deg, rgba(225, 86, 124, 0.24) 0%, rgba(108, 95, 199, 0.28) 100%)",
        border: "rgba(225, 86, 124, 0.45)",
        fg: "#ff7eb3",
        glyph: (
          <path
            d="M13.2 5.4a1.5 1.5 0 0 0-2.6 0L4.8 15.5a1.5 1.5 0 0 0 1.3 2.2h2.4v-1.8H6.9l5-8.8 4.9 8.8h-2.1v1.8h2.5a1.5 1.5 0 0 0 1.3-2.2L13.2 5.4Zm-1.3 5.2a4.6 4.6 0 0 0-3.9 4.5h1.8a2.8 2.8 0 0 1 2.8-2.7v-1.8h-.7Z"
            fill="currentColor"
          />
        ),
      };
    case "supabase":
      return {
        bg: "linear-gradient(135deg, rgba(62, 207, 142, 0.24) 0%, rgba(16, 120, 76, 0.28) 100%)",
        border: "rgba(62, 207, 142, 0.46)",
        fg: "#3ecf8e",
        glyph: (
          <path
            d="M13.2 4.5L6.2 13.3h5.2l-.6 6.2 7-8.8h-5.2l.6-6.2Z"
            fill="currentColor"
          />
        ),
      };
    case "vercel":
      return {
        bg: "linear-gradient(135deg, #262626 0%, #111111 100%)",
        border: "rgba(120, 120, 120, 0.45)",
        fg: "#ffffff",
        glyph: <path d="M12 5.2L19.4 18H4.6L12 5.2Z" fill="currentColor" />,
      };
    case "figma":
      return {
        bg: "linear-gradient(135deg, rgba(242, 78, 30, 0.24) 0%, rgba(162, 89, 255, 0.26) 100%)",
        border: "rgba(162, 89, 255, 0.45)",
        fg: "#ff8566",
        glyph: (
          <>
            <rect x="7.5" y="4.5" width="4.5" height="4.5" rx="2.2" fill="#F24E1E" />
            <rect x="12" y="4.5" width="4.5" height="4.5" rx="2.2" fill="#FF7262" />
            <rect x="7.5" y="9" width="4.5" height="4.5" rx="2.2" fill="#A259FF" />
            <circle cx="14.25" cy="11.25" r="2.25" fill="#1ABCFE" />
            <path
              d="M7.5 15.75a2.25 2.25 0 0 1 2.25-2.25H12v2.25a2.25 2.25 0 1 1-4.5 0Z"
              fill="#0ACF83"
            />
          </>
        ),
      };
    case "atlassian":
    case "trello":
      return {
        bg: "linear-gradient(135deg, rgba(38, 132, 255, 0.26) 0%, rgba(0, 82, 204, 0.3) 100%)",
        border: "rgba(76, 154, 255, 0.46)",
        fg: "#579dff",
        glyph:
          key === "trello" ? (
            <rect
              x="5.5"
              y="5.5"
              width="13"
              height="13"
              rx="2.5"
              stroke="currentColor"
              strokeWidth="1.8"
            />
          ) : (
            <path
              d="M9.6 10.6c-.4-.5-1.1-.5-1.4.1L5 17.2a.8.8 0 0 0 .7 1.1h4.6c.4 0 .7-.2.9-.6 1-2-.2-5.2-1.6-7.1Zm3.1-5.2c-.4-.6-1.2-.6-1.5 0-1.8 3.3-2 6.8-.5 9.8l1.6 2.5c.2.4.5.6.9.6h4.7a.8.8 0 0 0 .7-1.1L12.7 5.4Z"
              fill="currentColor"
            />
          ),
      };
    case "cloudflare":
      return {
        bg: "linear-gradient(135deg, rgba(246, 130, 31, 0.25) 0%, rgba(251, 173, 65, 0.2) 100%)",
        border: "rgba(246, 130, 31, 0.48)",
        fg: "#f6821f",
        glyph: (
          <path
            d="M16.2 15.5H7.6a3.1 3.1 0 0 1-.4-6.2 4.6 4.6 0 0 1 8.8-1.1 3.4 3.4 0 0 1 .2 7.3Z"
            fill="currentColor"
          />
        ),
      };
    case "context7":
      return {
        bg: "linear-gradient(135deg, rgba(16, 185, 129, 0.25) 0%, rgba(5, 150, 105, 0.25) 100%)",
        border: "rgba(52, 211, 153, 0.45)",
        fg: "#34d399",
        glyph: (
          <>
            <path
              d="M9.5 8.5H7.2a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h2.3"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            />
            <path
              d="M12.5 8.5h5.2l-3.2 7"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </>
        ),
      };
    case "deepwiki":
      return {
        bg: "linear-gradient(135deg, rgba(56, 189, 248, 0.24) 0%, rgba(14, 116, 144, 0.28) 100%)",
        border: "rgba(56, 189, 248, 0.45)",
        fg: "#38bdf8",
        glyph: (
          <>
            <path
              d="M5.5 7.2A2.2 2.2 0 0 1 7.7 5H12v13H7.7a2.2 2.2 0 0 0-2.2 1.2V7.2Zm13 0A2.2 2.2 0 0 0 16.3 5H12v13h4.3a2.2 2.2 0 0 1 2.2 1.2V7.2Z"
              stroke="currentColor"
              strokeWidth="1.7"
              strokeLinejoin="round"
            />
          </>
        ),
      };
    case "gitlab":
      return {
        bg: "linear-gradient(135deg, rgba(252, 109, 38, 0.26) 0%, rgba(226, 67, 41, 0.26) 100%)",
        border: "rgba(252, 109, 38, 0.48)",
        fg: "#fc6d26",
        glyph: (
          <path
            d="M12 18.2L19 13l-1.9-6.6a.5.5 0 0 0-.96 0L14.5 11h-5L7.86 6.4a.5.5 0 0 0-.96 0L5 13l7 5.2Z"
            fill="currentColor"
          />
        ),
      };
    case "huggingface":
      return {
        bg: "linear-gradient(135deg, rgba(255, 210, 30, 0.24) 0%, rgba(245, 158, 11, 0.24) 100%)",
        border: "rgba(255, 210, 30, 0.46)",
        fg: "#ffd21e",
        glyph: (
          <>
            <circle cx="12" cy="12" r="6.8" stroke="currentColor" strokeWidth="1.7" />
            <circle cx="9.6" cy="10.5" r="1" fill="currentColor" />
            <circle cx="14.4" cy="10.5" r="1" fill="currentColor" />
            <path
              d="M9.2 13.6c.7 1.1 1.7 1.6 2.8 1.6s2.1-.5 2.8-1.6"
              stroke="currentColor"
              strokeWidth="1.6"
              strokeLinecap="round"
            />
          </>
        ),
      };
    case "postman":
      return {
        bg: "linear-gradient(135deg, rgba(255, 108, 55, 0.25) 0%, rgba(234, 88, 12, 0.25) 100%)",
        border: "rgba(255, 108, 55, 0.46)",
        fg: "#ff6c37",
        glyph: (
          <>
            <circle cx="12" cy="12" r="6.8" stroke="currentColor" strokeWidth="1.7" />
            <path
              d="M9 15l6-6M11.5 9H15v3.5"
              stroke="currentColor"
              strokeWidth="1.7"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </>
        ),
      };
    case "neon":
    case "mongodb":
    case "prisma":
    case "posthog":
      return {
        bg: "linear-gradient(135deg, rgba(0, 229, 153, 0.22) 0%, rgba(20, 184, 166, 0.24) 100%)",
        border: "rgba(0, 229, 153, 0.44)",
        fg: "#00e599",
        glyph: (
          <>
            <ellipse cx="12" cy="7.5" rx="6" ry="2.5" stroke="currentColor" strokeWidth="1.7" />
            <path
              d="M6 7.5v9c0 1.4 2.7 2.5 6 2.5s6-1.1 6-2.5v-9M6 12c0 1.4 2.7 2.5 6 2.5s6-1.1 6-2.5"
              stroke="currentColor"
              strokeWidth="1.7"
            />
          </>
        ),
      };
    case "slack":
      return {
        bg: "linear-gradient(135deg, rgba(224, 30, 90, 0.24) 0%, rgba(54, 197, 240, 0.24) 100%)",
        border: "rgba(236, 178, 46, 0.45)",
        fg: "#ecb22e",
        glyph: (
          <path
            d="M9.5 6.5v11M14.5 6.5v11M6.5 9.5h11M6.5 14.5h11"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
          />
        ),
      };
    default: {
      const clean = rawLabel.replace(/[^a-zA-Z0-9]/g, "").toUpperCase();
      const mono = clean.slice(0, 2) || "MC";
      return {
        bg: "linear-gradient(135deg, var(--amber-soft), rgba(255, 255, 255, 0.04))",
        border: "var(--amber-border)",
        fg: "var(--amber)",
        glyph: (
          <text
            x="12"
            y="15.2"
            textAnchor="middle"
            fontSize="8.8"
            fontWeight="800"
            fontFamily="var(--mono)"
            fill="currentColor"
          >
            {mono}
          </text>
        ),
      };
    }
  }
}

export function ServiceIcon({
  slug,
  name,
  size = 24,
  className = "",
}: {
  slug?: string;
  name?: string;
  size?: number;
  className?: string;
}) {
  const key = resolveServiceKey(`${slug ?? ""} ${name ?? ""}`);
  const palette = getServicePalette(key, name || slug || "MCP");

  return (
    <span
      className={`service-icon-tile ${className}`}
      style={{
        width: size,
        height: size,
        background: palette.bg,
        borderColor: palette.border,
        color: palette.fg,
      }}
      aria-hidden="true"
    >
      <svg
        width={Math.round(size * 0.78)}
        height={Math.round(size * 0.78)}
        viewBox="0 0 24 24"
        fill="none"
      >
        {palette.glyph}
      </svg>
    </span>
  );
}

/**
 * Crisp SVG Alert Badge Icon for missing required parameters (replaces literal "?!" text).
 */
export function RequiredAlertIcon({
  size = 13,
  className = "",
}: {
  size?: number;
  className?: string;
}) {
  return (
    <svg
      className={`req-alert-svg ${className}`}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M8.57 3.22a1.65 1.65 0 0 1 2.86 0l6.18 10.9A1.65 1.65 0 0 1 16.18 16.6H3.82a1.65 1.65 0 0 1-1.43-2.48l6.18-10.9Z"
        fill="currentColor"
        fillOpacity="0.18"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinejoin="round"
      />
      <path
        d="M10 7.4v3.8"
        stroke="currentColor"
        strokeWidth="1.9"
        strokeLinecap="round"
      />
      <circle cx="10" cy="13.7" r="1.05" fill="currentColor" />
    </svg>
  );
}

export function RequiredCheckIcon({
  size = 12,
  className = "",
}: {
  size?: number;
  className?: string;
}) {
  return (
    <svg
      className={`req-check-svg ${className}`}
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      aria-hidden="true"
    >
      <circle
        cx="10"
        cy="10"
        r="7.5"
        fill="currentColor"
        fillOpacity="0.16"
        stroke="currentColor"
        strokeWidth="1.7"
      />
      <path
        d="M6.8 10.2l2.1 2.1 4.4-4.5"
        stroke="currentColor"
        strokeWidth="1.9"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
