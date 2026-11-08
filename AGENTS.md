# Loopback MCP Proxy — Workspace Agent Rules

`loopback-mcp-proxy` is a single-binary Rust + React local MCP credential isolation proxy. It seals upstream MCP credentials (OAuth 2.0 PKCE / Client Credentials, Bearer tokens, HTTP Basic, Custom Headers) inside an AES-256-GCM encrypted SQLite vault (`{DATA_DIR}/proxy.db`) so local CLI tools can use `http://127.0.0.1:8080/mcp` and `http://127.0.0.1:8080/mcp/{slug}` without storing secrets on the host.

## Project-Scoped Skills (`.agents/skills/`)

Always activate and follow the corresponding project skill before working on its domain:

1. **[`loopback-platform-release`](./.agents/skills/loopback-platform-release/SKILL.md)**
   - Activate when touching [`Makefile`](./Makefile), [`Dockerfile`](./Dockerfile), [`build.rs`](./build.rs), [`.cargo/config.toml`](./.cargo/config.toml), [`scripts/release-musl.sh`](./scripts/release-musl.sh), or running build/release/packaging workflows (`docs/platforms.md`).
2. **[`loopback-security-mcp-guardrails`](./.agents/skills/loopback-security-mcp-guardrails/SKILL.md)**
   - Activate when modifying Rust backend modules (`src/*.rs`) covering credential vault encryption, origin-bound secret reuse protection, `/mcp` and `/mcp/{slug}` routing, OAuth refresh, or Web/TUI real-time WebSocket sync (`docs/requirements.md`, `docs/design.md`).
3. **[`loopback-web-ux-standards`](./.agents/skills/loopback-web-ux-standards/SKILL.md)**
   - Activate when modifying the React Web UI (`web/src/*`) to maintain mandatory async `<Spinner />` + elapsed `ms` timers, macOS spring animations, pre-save Swagger tool execution (`/api/probe/call`), `<LiveTerminalGadget />`, and `<CodeViewer />` (Pretty JSON / YAML / Raw + Copy).

## Core Non-Negotiable Rules

- **Platform Order (`docs/platforms.md`)**: Linux (`amd64`, `arm64`) static `musl` (`+crt-static`) via `scripts/release-musl.sh` is Priority 1 (`build/linux-<arch>/release/loopback-mcp-proxy`). Do not create separate Windows or macOS build recipes ahead of the Linux release sync. Keep `cargo test` and `cargo run` on the host toolchain.
- **Zero Secret Leakage**: Never expose raw secrets in `ServerView`/`AuthPublic`, logs, or `ActivityEntry` WebSocket events. Reject blank-secret reuse when the upstream URL origin (`scheme + host + port`) or `auth.kind` changes.
- **Verification**: After any code change, run `./.agents/skills/loopback-platform-release/scripts/verify-all.sh` (which runs `npx tsc --noEmit`, `npm run build` into `web/dist`, and `cargo test`).
