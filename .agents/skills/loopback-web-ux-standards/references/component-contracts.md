# Shared Web UI Component Contracts (`web/src/ui.tsx` & `web/src/bench.tsx`)

## 1. `<Spinner />`
```tsx
<Spinner size={14} tone="current" | "amber" | "green" | "muted" | "dark" />
```
- Use `tone="dark"` inside primary amber buttons (`.btn.primary`).
- Use `tone="amber"` inside `.btn`, `.btn-ghost`, status pills, and terminal headers.

## 2. `<CopyButton />`
```tsx
<CopyButton
  text={stringToCopy}
  label="Copy"
  copiedLabel="✓ Copied"
  className="btn-ghost compact"
  onCopied={() => onNotify?.("Copied to clipboard")}
/>
```

## 3. `<CodeViewer />`
```tsx
<CodeViewer
  data={structuredObjectOrMcpResponse}
  rawText={optionalRawJsonString}
  defaultMode="json" // "json" | "yaml" | "raw"
  title={optionalReactNode}
  statusPill={optionalReactNode}
  durationMs={optionalNumber}
  loading={isExecuting}
  loadingLabel="Calling tool…"
  maxHeight={420}
  onNotify={onNotify}
/>
```

## 4. `<LiveTerminalGadget />`
```tsx
<LiveTerminalGadget
  title="mcp-terminal — /mcp/github"
  entries={activityEntries}
  filterSlug="github" // omit for global stream
  busy={isOperationRunning}
  busyText="Probing upstream…"
  defaultExpanded={true}
  maxHeight={280}
  onClear={onClearActivity}
  onNotify={onNotify}
/>
```

## 5. `<ToolBench />`
Supports both saved servers (`server?: ServerView`) and pre-save wizard probe sessions (`probeInput?: ServerInput`):
```tsx
// Saved server mode (Detail page):
<ToolBench server={server} onNotify={onNotify} />

// Ephemeral pre-save probe mode (ServerForm wizard):
<ToolBench
  compact
  tools={probe.tools}
  slug={slug || "probe"}
  probeInput={payload()}
  initialToolName={selectedProbeTool}
  onNotify={onNotify}
/>
```
