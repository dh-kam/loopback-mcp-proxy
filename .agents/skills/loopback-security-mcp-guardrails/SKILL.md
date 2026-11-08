---
name: loopback-security-mcp-guardrails
description: >-
  Enforces credential vault isolation, origin-bound secret reuse protection,
  AES-256-GCM encryption, log/telemetry secret redaction, downstream MCP
  endpoint routing (/mcp and /mcp/{slug}), and Web/TUI real-time sync parity
  for loopback-mcp-proxy. Use this skill whenever modifying Rust backend files
  (src/model.rs, src/store.rs, src/crypto.rs, src/auth.rs, src/catalog.rs,
  src/mcp.rs, src/http.rs, src/tui.rs, src/flow_test.rs).
---

# Loopback Security & MCP Protocol Guardrails

`loopback-mcp-proxy` exists to isolate upstream MCP credentials inside an encrypted container vault (`{DATA_DIR}/proxy.db` + `master.key` / `PROXY_DATA_KEY`) so local CLI tools (`claude`, Cursor, Gemini CLI, etc.) can connect to `http://127.0.0.1:8080/mcp` or `http://127.0.0.1:8080/mcp/{slug}` without storing API keys, JWTs, or OAuth tokens on the host filesystem.

Every backend modification must preserve the following security and protocol invariants.

## 1. Credential Isolation & Zero Secret Leakage

1. **Never Return Secrets in Public Views**:
   - `ServerView` and `AuthPublic` ([`src/model.rs`](../../../src/model.rs)) must never serialize raw secret values (`bearerToken`, `basicPassword`, `headers[].value`, `clientSecret`, `accessToken`, `refreshToken`).
   - Only expose boolean flags or metadata (`bearerConfigured`, `headerNames`, `hasClientSecret`, `hasAccessToken`, `hasRefreshToken`, `expiresAt`).
2. **Origin-Bound Secret Reuse Prevention (Anti-Exfiltration)**:
   - When editing a server (`PUT /api/servers/{id}`) or probing (`POST /api/probe`, `POST /api/probe/call` with `serverId`), blank secret inputs may only reuse stored vault credentials via `merge_server_auth` ([`src/model.rs`](../../../src/model.rs)) when **both** conditions hold:
     - `previous.auth.kind == input.auth.kind`
     - `same_origin(&previous.url, &input.url)` (`scheme + host + port` match)
   - If the upstream URL origin changes and a required secret is left blank, `validate_server` must reject the request with `400 Bad Request`. Never send stored credentials to a new origin.
3. **Telemetry & Log Redaction**:
   - All `ActivityEntry` payloads (`requestPreview`, `responsePreview`) emitted by [`src/catalog.rs`](../../../src/catalog.rs) or [`src/mcp.rs`](../../../src/mcp.rs) must pass through `redact_and_truncate_json`, which masks any JSON object key containing `token`, `secret`, `password`, `authorization`, `api_key`, `apikey`, or `credential`.
   - Never log `HeaderMap` authorization values or `AuthConfig` structs in tracing or activity streams.
4. **At-Rest Encryption & Loopback Binding**:
   - `servers.auth_blob` in SQLite must always be encrypted with AES-256-GCM ([`src/crypto.rs`](../../../src/crypto.rs)).
   - Key and DB files must set `0600` permissions on Unix.
   - Non-loopback `BIND` addresses must be rejected unless `ALLOW_NON_LOOPBACK=1` is set ([`src/config.rs`](../../../src/config.rs)).
   - If `ADMIN_TOKEN` is set, `/api/*` and `/api/ws` require `Authorization: Bearer <ADMIN_TOKEN>` (or `?token=` on WebSocket), while `/mcp`, `/mcp/{slug}`, and `/oauth/callback` remain open for local MCP clients and browser OAuth redirects.

## 2. Downstream & Upstream MCP Protocol Rules

1. **Slug & Tool Namespace Rules**:
   - Server `slug` must be `1..=48` characters of `[a-z0-9-]` with no leading/trailing/consecutive hyphens and **no underscores (`_`)**.
   - Aggregated `/mcp` namespaces tools as `{slug}__{tool_name}`, splitting on the **first** `__` so underscores (including `__`) inside the upstream `tool_name` remain intact (`split_namespaced_tool`).
2. **Scoped Endpoint (`/mcp/{slug}`) Isolation**:
   - `POST /mcp/{slug}` exposes the single upstream's original tool names (without `{slug}__` prefix), returns the upstream's `serverInfo` and scoped `instructions`, and binds the created `mcp-session-id` to `Some(slug)`.
   - A session created on `/mcp/{slug}` must be rejected if reused on `/mcp` or another `/mcp/{other}`, and vice versa.
   - `GET /mcp/{slug}` SSE stream must only emit `notifications/tools/list_changed` when events affect that specific `slug` (`scope_matches_event`).
3. **OAuth 2.0 Lifecycle**:
   - Support both `authorization_code` (with S256 PKCE, dynamic client registration fallback, and `oauth_tx` 15-minute TTL pruning) and `client_credentials`.
   - Automatically refresh tokens when within 60 seconds of expiry (`expires_within(60)`), and perform one forced token refresh + retry on upstream HTTP `401`/`403`.

## 3. Web & TUI Real-Time Sync Parity

- Every catalog mutation (`upsert`, `remove`, `activity`) must broadcast via `Catalog.bus` (`BusEvent`) over `/api/ws`.
- Whenever a new variant is added to `BusEvent` ([`src/model.rs`](../../../src/model.rs)), update both:
  - [`src/http.rs`](../../../src/http.rs) (`ws_loop` serialization)
  - [`src/tui.rs`](../../../src/tui.rs) (`WsMessage` deserialization) so the TUI never drops or fails on unknown WebSocket frames.

## 4. Verification Checklist

See [security-checklist.md](./references/security-checklist.md) for required test coverage in [`src/flow_test.rs`](../../../src/flow_test.rs) and [`src/model.rs`](../../../src/model.rs).
