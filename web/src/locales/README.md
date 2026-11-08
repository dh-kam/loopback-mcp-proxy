# Loopback MCP Proxy — Internationalization (i18n) Contributor Guide

All UI translation strings for `loopback-mcp-proxy` are isolated as standalone, flat JSON dictionaries inside `web/src/locales/`. This allows any developer or community contributor to add a new language or refine existing translations without touching React component logic.

## Built-in Locales

| Code | Language | Native Name | File |
| :--- | :--- | :--- | :--- |
| `en` | English (Source / Fallback) | English | [`en.json`](./en.json) |
| `ko` | Korean | 한국어 | [`ko.json`](./ko.json) |
| `ja` | Japanese | 日本語 | [`ja.json`](./ja.json) |
| `es` | Spanish | Español | [`es.json`](./es.json) |

## How to Contribute a New Language

1. **Copy the canonical English resource file**:
   ```bash
   cp web/src/locales/en.json web/src/locales/<lang-code>.json
   ```
   *(For example, `fr.json` for French, `de.json` for German, or `zh.json` for Chinese.)*

2. **Translate the values in `<lang-code>.json`**:
   - Keep all JSON keys identical to `en.json` (e.g., `"bench.executeBtn"`, `"home.heroTitle"`).
   - Preserve `{placeholder}` interpolation tokens such as `{count}`, `{total}`, `{missing}`, `{ms}`, `{tool}`, and `{name}`.
   - Technical terms like `MCP`, `OAuth 2.0`, `PKCE`, `AES-256-GCM`, `JSON-RPC`, `Slug`, and `cURL` should remain recognizable to developers.
   - If any key is omitted in a locale file, the i18n runtime ([`web/src/i18n.tsx`](../i18n.tsx)) automatically falls back to `en.json` so the UI never renders broken keys.

3. **Register the locale in [`web/src/i18n.tsx`](../i18n.tsx)**:
   Import your JSON file and add an entry to `SUPPORTED_LOCALES`:
   ```ts
   import fr from "./locales/fr.json";

   export const SUPPORTED_LOCALES: LocaleMeta[] = [
     // ...existing locales
     { code: "fr", label: "French", nativeLabel: "Français", shortBadge: "FR", messages: fr },
   ];
   ```

4. **Verify TypeScript & Build**:
   ```bash
   ./.agents/skills/loopback-platform-release/scripts/verify-all.sh
   ```
