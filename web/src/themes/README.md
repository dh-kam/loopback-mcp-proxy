# Loopback MCP Proxy — Pluggable Theme System (`web/src/themes/`)

Loopback MCP Proxy features a CSS-token-driven **Theme System** that supports both **built-in themes** (`web/src/themes/*.json`) and **user-installable runtime Theme Plugins** (imported via JSON or `.theme.json` files directly in the Web UI).

---

## 1. Built-in Themes

| File | ID | Name | Appearance |
|------|----|------|------------|
| [`obsidian-amber.json`](./obsidian-amber.json) | `obsidian-amber` | **Obsidian & Amber** (Default) | `dark` |
| [`midnight-cyan.json`](./midnight-cyan.json) | `midnight-cyan` | **Midnight Cyber** | `dark` |
| [`catppuccin-mocha.json`](./catppuccin-mocha.json) | `catppuccin-mocha` | **Catppuccin Mocha** | `dark` |
| [`nordic-frost.json`](./nordic-frost.json) | `nordic-frost` | **Nordic Frost** | `dark` |
| [`daylight-paper.json`](./daylight-paper.json) | `daylight-paper` | **Daylight Paper** | `light` |
| [`github-light.json`](./github-light.json) | `github-light` | **GitHub Frost Light** | `light` |
| [`catppuccin-latte.json`](./catppuccin-latte.json) | `catppuccin-latte` | **Catppuccin Latte** | `light` |
| [`solarized-light.json`](./solarized-light.json) | `solarized-light` | **Solarized Parchment** | `light` |
| [`rose-pine-dawn.json`](./rose-pine-dawn.json) | `rose-pine-dawn` | **Rosé Pine Dawn** | `light` |

---

## 2. Creating a Custom Theme Plugin (`.theme.json`)

Anyone can create a theme plugin without recompiling the project:

1. Click the **🎨 Theme Switcher** in the top navigation bar and select **`+ Theme Plugin Studio`**.
2. Paste or upload a `.theme.json` manifest following this schema:

```json
{
  "id": "tokyo-night",
  "name": "Tokyo Night",
  "author": "your-github-handle",
  "version": "1.0.0",
  "appearance": "dark",
  "description": "Clean Tokyo Night dark theme with neon blue & magenta highlights.",
  "preview": ["#1a1b26", "#7aa2f7", "#9ece6a"],
  "tokens": {
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
    "--muted": "#565f89",
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
    "--shadow-card": "0 12px 32px rgba(10, 10, 15, 0.5)"
  },
  "customCss": ""
}
```

3. Click **Install & Activate Plugin**. The theme is saved in browser storage (`loopback_theme_plugins`) and can also be exported as a `.theme.json` file to share with others.

---

## 3. Contributing a Built-in Theme via Pull Request

1. Add your `<theme-id>.json` file to `web/src/themes/`.
2. Import and register it in `BUILTIN_THEMES` inside [`web/src/theme.tsx`](../theme.tsx).
3. Run `./.agents/skills/loopback-platform-release/scripts/verify-all.sh` to verify the build.
