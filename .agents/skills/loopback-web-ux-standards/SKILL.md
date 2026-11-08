---
name: loopback-web-ux-standards
description: >-
  Enforces the commercial-grade Web UI/UX standards for loopback-mcp-proxy,
  including mandatory SVG spinners with live elapsed timers on all async
  actions, macOS-style spring animations, pre-save Swagger tool execution after
  connection tests, the real-time LiveTerminalGadget, and Pretty JSON/YAML/Raw
  CodeViewer formatting. Use this skill whenever modifying files in web/src/.
---

# Loopback Web UI/UX & Motion Design Standards

The `loopback-mcp-proxy` Web UI (`web/src/`) follows the **"Obsidian Vault & Amber Phosphor"** industrial-grade aesthetic combined with macOS-Fluid spring motion and real-time MCP observability. Whenever adding or modifying Web UI features, adhere to these four pillars.

## 1. Mandatory Spinners + Live Elapsed Timers (`ms`)

Every operation that can take noticeable time (network request, upstream MCP handshake, OAuth discovery, tool execution, database save/delete) **must**:
1. Track a granular busy state (e.g., `busyAction: "discover" | "probe" | "save" | "reconnect" | "toggle" | "authorize" | "delete" | null` or per-card `busyState`) so only the active trigger shows a spinner while conflicting actions are disabled.
2. Render the shared dual-layer SVG [`<Spinner />`](../../../web/src/ui.tsx) component inside the button or status pill.
3. Pair long-running network/RPC actions with [`useElapsedTimer(active)`](../../../web/src/ui.tsx) so the user sees live millisecond progress (e.g., `Probing (140ms)…`, `Executing (85ms)…`) before smoothly transitioning to the result.

## 2. Unified macOS Spring Animation System

All content appearances, disappearances, and view transitions must use the shared spring physics curves defined in [`web/src/styles.css`](../../../web/src/styles.css):
- `--spring-snappy: cubic-bezier(0.22, 1, 0.36, 1)` (tabs, step transitions, drawers, hover lift)
- `--spring-bouncy: cubic-bezier(0.34, 1.56, 0.64, 1)` (modals, copy pop micro-feedback, toast banners)
- `--spring-smooth: cubic-bezier(0.16, 1, 0.3, 1)` (route-level page transitions)

### Required Animation Wrappers
- **Route Transitions**: Keep `<Routes>` wrapped in `<div key={location.pathname} className="route-stage">` in [`web/src/App.tsx`](../../../web/src/App.tsx).
- **Wizard Step Transitions**: Compute navigation direction (`"forward"` vs `"backward"`) and apply `className="wizard-body step-anim-forward"` or `step-anim-backward` in [`web/src/form.tsx`](../../../web/src/form.tsx).
- **Drawers / Expandable Panels**: Use `.collapsible-drawer` + `.collapsible-inner` (`grid-template-rows: 0fr -> 1fr`) instead of abrupt mount/unmount jumps.
- **Callouts & Mode Switches**: Apply `.callout-anim` to dynamic status banners and `.mode-switch-anim` (with a `key` tied to the active tab/mode) when switching tabs or tool views.
- **Reduced Motion**: Always preserve the `@media (prefers-reduced-motion: reduce)` rule in `web/src/styles.css`.

## 3. Swagger Tool Execution Right After Connection Test + Live Terminal Gadget

1. **Pre-Save Tool Execution (`POST /api/probe/call`)**:
   - In [`ServerForm`](../../../web/src/form.tsx), as soon as a connection test (`Probe`) succeeds and returns tools (`probe.ok && probe.tools.length > 0`), render interactive tool chips and an inline `<ToolBench compact tools={probe.tools} slug={slug} probeInput={payload()} />` drawer so operators can invoke discovered tools immediately before saving.
2. **Required Parameter Guardrail (`?!` Indicator + Auto-Focus + Warning Toast)**:
   - In [`ToolBench`](../../../web/src/bench.tsx), compute missing `required` fields in real time via `getMissingRequiredFields(schema, effectiveArgs)`.
   - Keep `▶ Execute Tool` clickable even when required fields are empty, but style it with `.btn.primary.has-missing-req` and an inline `?! N` badge (`.exec-missing-badge`) alongside a `?! N/M required empty` status chip.
   - Clicking `▶ Execute Tool` while required fields are empty must smoothly scroll and focus the first empty required field (`focusFieldByKey`), trigger a warning toast (`?! Missing required parameter(s)…`), shake-highlight missing field cards (`.is-guard-alert`), and display an inline guard bar with `✨ Auto-Fill Missing` and `▶ Force Execute Anyway`.
3. **Real-Time Terminal Gadget (`<LiveTerminalGadget />`)**:
   - Render [`<LiveTerminalGadget />`](../../../web/src/ui.tsx) in:
     - **Overview (`Home`)**: Global MCP telemetry stream across all upstreams.
     - **Server Detail (`Detail`)**: Scoped stream (`filterSlug={server.slug}`) alongside `<ToolBench />`.
     - **Setup/Edit Wizard (`ServerForm`)**: Probe & pre-save tool execution stream (`source === "probe"` or matching `slug`).
   - Support source filtering (`all`, `mcp`, `admin`, `probe`, `system`), `Auto-scroll` toggle, `Copy Log`, `Clear`, and clicking any log line with payloads to expand inline redacted request/response `<CodeViewer />` drawers.

## 4. Pretty JSON / YAML Syntax Highlighting + Raw Toggle + Copy (`<CodeViewer />`)

1. **Never Dump Unformatted JSON**:
   - Render all MCP tool responses, terminal payload previews, and JSON client configuration snippets using [`<CodeViewer />`](../../../web/src/ui.tsx).
2. **Format Modes (`Pretty JSON` | `YAML` | `Raw`)**:
   - Default to **Pretty JSON** with automatic inner-JSON unwrapping (`unwrapMcpPayload` parses JSON strings inside MCP `content[].text` and displays an `Auto-Unwrapped` badge) and token-level syntax highlighting (`highlightJson`).
   - Provide one-click switching to syntax-highlighted **YAML** (`jsonToYaml` + `highlightYaml`) and untouched **Raw** payload view.
3. **Copy Micro-Interactions (`<CopyButton />`)**:
   - Every `<CodeViewer />`, endpoint URL, CLI command snippet, and tool name badge must include a [`<CopyButton />`](../../../web/src/ui.tsx) that transitions to `✓ Copied` with spring pop feedback.

See [component-contracts.md](./references/component-contracts.md) for the shared prop contracts in `web/src/ui.tsx` and `web/src/bench.tsx`.
