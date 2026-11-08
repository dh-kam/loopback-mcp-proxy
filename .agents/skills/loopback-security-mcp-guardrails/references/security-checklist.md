# Security & Protocol Verification Checklist

Before completing any backend change in `src/`, verify the following tests pass via `cargo test`:

1. **Unit Tests (`src/model.rs`, `src/crypto.rs`, `src/auth.rs`, `src/config.rs`, `src/sse.rs`)**:
   - `model::tests::origin_change_prevents_secret_reuse`: Verifies `same_origin` and `merge_server_auth` drop stored secrets when scheme, host, or port changes.
   - `model::tests::merge_keeps_blank_secrets`: Verifies blank secrets are preserved when origin and auth kind remain identical.
   - `model::tests::slug_rules` & `model::tests::namespaced_split_keeps_tool_underscores`: Verifies slug constraints and `{slug}__{tool}` splitting.
   - `crypto::tests::round_trip`: Verifies AES-256-GCM encryption/decryption and tamper rejection.
   - `config::tests::admin_token_must_match` & `open_when_admin_token_unset`: Verifies admin boundary.

2. **Integration Tests (`src/flow_test.rs`)**:
   - `flow_test::bearer_proxy_hides_credentials_and_calls_tools`:
     - Verifies `/api/probe`, `/api/servers`, `AuthPublic` secret hiding, and blank secret retention on same-origin update.
     - Verifies that changing upstream URL origin with blank secret returns `400 Bad Request` on `/api/probe`, `PUT /api/servers/{id}`, and `/api/probe/call` without leaking the Bearer token to the new host.
     - Verifies `/api/probe/call` executes tools ephemerally before/without modifying stored state.
     - Verifies `/mcp` (namespaced `lab__echo`) and `/mcp/lab` (unprefixed `echo` + scope isolation) both work without client credentials.
   - `flow_test::oauth_client_credentials_refresh_before_tool_call`:
     - Verifies OAuth 2.0 `client_credentials` token acquisition, expiry refresh, and authenticated upstream tool execution.
