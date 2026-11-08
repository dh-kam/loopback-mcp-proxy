# loopback-mcp-proxy

A local proxy that connects to remote MCP servers and keeps their credentials. MCP clients on the loopback interface talk to one unauthenticated endpoint.

```mermaid
flowchart LR
  client[MCP client]
  proxy[loopback-mcp-proxy]
  upstream[Remote MCP servers]
  client -->|"127.0.0.1/mcp no credentials"| proxy
  proxy -->|"OAuth, Bearer, Basic, or custom headers"| upstream
```

The proxy stores upstream credentials, refreshes OAuth tokens, and exposes every connected server's tools. The aggregated endpoint namespaces tools as `{slug}__{tool}`. A per-server endpoint is available at `/mcp/{slug}`.

## Run

```bash
docker compose up -d --build
```

The compose file publishes the service on `127.0.0.1:8080` only. Inside the container the process listens on `0.0.0.0` so Docker can forward the published port. A non-loopback bind is refused unless `ALLOW_NON_LOOPBACK=1`.

- Web UI: `http://127.0.0.1:8080/`
- Aggregated MCP: `http://127.0.0.1:8080/mcp`
- One upstream: `http://127.0.0.1:8080/mcp/{slug}`
- Admin TUI: `docker exec -it <container> loopback-mcp-proxy tui`

Persistent state is the `proxy-data` volume: an encrypted SQLite database and `master.key`. Set `PROXY_DATA_KEY` to a 32-byte standard-base64 key if the key should live outside the volume. Set `ADMIN_TOKEN` to require a bearer token on the admin API and WebSocket. The MCP endpoints stay open so local clients do not handle credentials.

Example client configuration:

```json
{
  "mcpServers": {
    "loopback": {
      "url": "http://127.0.0.1:8080/mcp"
    }
  }
}
```

## Authentication

| Kind | What the proxy sends |
| --- | --- |
| None | No credential header |
| Bearer | `Authorization: Bearer <token>` |
| Basic | `Authorization: Basic ...`. Realm is recorded and compared with the server challenge. |
| Headers | The configured request headers |
| OAuth 2 | Bearer access token. Authorization-code grants use PKCE and refresh tokens. Client-credentials grants request a new token when the current one is inside 60 seconds of expiry. A `401` forces one refresh and a retry. |

The add wizard can discover OAuth metadata from an unauthenticated `initialize` and from `WWW-Authenticate`. If the authorization server exposes dynamic client registration and the client id is empty, the proxy registers itself before opening the browser.

## Web and TUI

Both subscribe to `/api/ws`. Adding, editing, deleting, or reconnecting a server updates every open UI. The web wizard tests `initialize` and `tools/list` before save. The tool bench renders `inputSchema` and executes `tools/call` through the proxy. The TUI (`a` add, `e` edit, `d` delete, `r` reconnect, `ctrl-t` test, `ctrl-s` save) is for server management; tool execution stays in the web UI.

## Development

```bash
cargo test
make web
cargo run -- serve
cd web && npm run dev
make build
```

`npm run dev` serves the UI on port 5173 and proxies the API. For an OAuth redirect back to Vite, set `PUBLIC_BASE_URL=http://127.0.0.1:5173`.

`make build` (also `make linux`) produces the native Linux release. `make linux-amd64` and `make linux-arm64` select the architecture. Both use musl with `crt-static`, so the binary has no shared libc. The artifact is `build/linux-<arch>/release/loopback-mcp-proxy`. A native build needs `musl-gcc` from the `musl-tools` package. A cross build needs `<rust-arch>-linux-musl-gcc`, or `MUSL_CC`. `cargo run` and `cargo test` stay on the host toolchain. The Docker image copies this same static binary.

Windows and macOS releases are not built yet. They will follow this Linux release recipe after it, rather than a separate build design. The platform order and release rules are in `docs/platforms.md`.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `BIND` | `127.0.0.1:8080` | Listen address |
| `ALLOW_NON_LOOPBACK` | unset | Required to bind outside loopback |
| `DATA_DIR` | `./data` | Database and encryption key |
| `PUBLIC_BASE_URL` | `http://127.0.0.1:<port>` | OAuth redirect base |
| `ADMIN_TOKEN` | unset | Protects `/api` when set |
| `PROXY_DATA_KEY` | unset | 32-byte base64 key for credential encryption |
