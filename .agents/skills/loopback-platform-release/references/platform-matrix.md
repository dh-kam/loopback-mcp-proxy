# Platform Release Matrix & Future Sync Specification

Reference rules from [`docs/platforms.md`](../../../../docs/platforms.md).

## Current Active Targets (Linux Priority 1)

| OS | Normalized Arch | Rust Target Triple | C Compiler (Native / Cross) | Output Binary Path |
| :--- | :--- | :--- | :--- | :--- |
| `linux` | `amd64` | `x86_64-unknown-linux-musl` | `musl-gcc` / `x86_64-linux-musl-gcc` | `build/linux-amd64/release/loopback-mcp-proxy` |
| `linux` | `arm64` | `aarch64-unknown-linux-musl` | `musl-gcc` / `aarch64-linux-musl-gcc` | `build/linux-arm64/release/loopback-mcp-proxy` |

## Future Synchronization Targets (Phase 2 — Do Not Implement Prematurely)

When Windows and macOS targets are synchronized from the Linux release recipe, they must adhere to the exact same layout:

| OS | Normalized Arch | Expected Rust Target Triple | Output Binary Path |
| :--- | :--- | :--- | :--- |
| `darwin` | `amd64` | `x86_64-apple-darwin` | `build/darwin-amd64/release/loopback-mcp-proxy` |
| `darwin` | `arm64` | `aarch64-apple-darwin` | `build/darwin-arm64/release/loopback-mcp-proxy` |
| `windows` | `amd64` | `x86_64-pc-windows-msvc` | `build/windows-amd64/release/loopback-mcp-proxy.exe` |
| `windows` | `arm64` | `aarch64-pc-windows-msvc` | `build/windows-arm64/release/loopback-mcp-proxy.exe` |

### Synchronization Checklist
- Keep `amd64` and `arm64` as the only architecture directory suffixes under `build/<os>-<arch>/release/`.
- Ensure standalone execution without external runtime library installation wherever the OS permits (e.g., static CRT on Windows `+crt-static` if applicable, bundled SQLite via `rusqlite` `bundled` feature).
- Never alter `cargo test` or `cargo run` to require cross-compilers; keep developer commands on the native host toolchain.
