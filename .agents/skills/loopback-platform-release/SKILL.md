---
name: loopback-platform-release
description: >-
  Enforces platform priority, musl static linking, build artifact conventions,
  toolchain separation, and Docker packaging rules for loopback-mcp-proxy based
  on docs/platforms.md. Use this skill whenever modifying Makefile, Dockerfile,
  build.rs, Cargo.toml, .cargo/config.toml, scripts/release-musl.sh, or running
  build, test, release, or cross-platform synchronization tasks.
---

# Loopback Platform & Release Skill

This skill governs how `loopback-mcp-proxy` is built, tested, statically linked, and packaged across platforms (`docs/platforms.md`). Follow these rules strictly on every build or release task.

## 1. Platform Priority & Synchronization Order

1. **Linux (`amd64`, `arm64`) is Priority 1**:
   - The single canonical release recipe is [`scripts/release-musl.sh`](../../../scripts/release-musl.sh).
   - Both `amd64` (`x86_64-unknown-linux-musl`) and `arm64` (`aarch64-unknown-linux-musl`) share this script.
2. **No Divergent Windows / macOS Build Systems**:
   - Do **not** design or introduce separate build pipelines for `darwin` or `windows` ahead of the Linux release recipe.
   - When Windows or macOS targets are added later, they must synchronize with the Linux release structure and output conventions defined in [platform-matrix.md](./references/platform-matrix.md).

## 2. Mandatory Invariants

### A. Static Linking (`musl` + `crt-static`)
- Linux release binaries must run outside containers without any shared `libc` or `libgcc` dependency.
- Keep `target-feature=+crt-static` enabled for both `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` in [`.cargo/config.toml`](../../../.cargo/config.toml).
- Because `rusqlite` (`bundled`) compiles `sqlite3.c` via `cc`, `scripts/release-musl.sh` must resolve the C compiler (`musl-gcc` for native builds, `<rust-arch>-linux-musl-gcc` for cross builds, or `$MUSL_CC` override) and export `CC_<target_triple>`.

### B. Artifact Output Path Convention
- Release binaries must always be copied to:
  ```text
  build/<os>-<arch>/release/loopback-mcp-proxy[.exe]
  ```
- Architecture names in `build/` are strictly normalized to `amd64` and `arm64` (never `x86_64` or `aarch64`).

### C. Development vs. Release Toolchain Separation
- **Development (`cargo test`, `cargo run`)**: Always use the **host toolchain** (default target) for fast iteration. Never hardcode `build.target` in `.cargo/config.toml` or force `cargo test` onto the `musl` release target.
- **Release (`make build`, `make linux`, `make linux-amd64`, `make linux-arm64`)**: Always build web assets first (`make web`), then run `./scripts/release-musl.sh`.

### D. Embedded Web Assets (`web/dist`)
- [`src/web.rs`](../../../src/web.rs) embeds `web/dist` into the Rust binary using `rust-embed`.
- [`build.rs`](../../../build.rs) creates a minimal placeholder `web/dist/index.html` only so `cargo test` can compile on a clean checkout.
- Before any release binary build, Docker build, or end-to-end UI check, `web/dist` **must** be built via `cd web && npx tsc --noEmit && npm run build` (or `make web`).

### E. Container Packaging (`Dockerfile`)
- [`Dockerfile`](../../../Dockerfile) must use a 3-stage pipeline:
  1. `node:22-bookworm-slim` (`AS web`): runs `npm ci && npm run build`.
  2. `rust:1.94.1-bookworm` (`AS build`): installs `musl-tools`, copies `web/dist`, and executes `./scripts/release-musl.sh`.
  3. `debian:bookworm-slim`: installs `ca-certificates` only and copies the static binary to `/usr/local/bin/loopback-mcp-proxy` with `ENTRYPOINT ["loopback-mcp-proxy"]` and `CMD ["serve"]`.

## 3. Standard Recurring Verification Workflow

After making changes to Rust or Web code, run the verification helper script:

```bash
./.agents/skills/loopback-platform-release/scripts/verify-all.sh
```

Or for a full static musl release verification (when `musl-gcc` is installed):

```bash
./.agents/skills/loopback-platform-release/scripts/verify-all.sh --release
```
