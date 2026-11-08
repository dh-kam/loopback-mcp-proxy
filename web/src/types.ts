export type Transport = "streamable-http" | "sse";
export type AuthKind = "none" | "bearer" | "basic" | "headers" | "oauth2";
export type OAuthGrant = "authorization_code" | "client_credentials";
export type ConnState = "idle" | "disabled" | "connecting" | "connected" | "authRequired" | "error";

export interface HeaderPair {
  name: string;
  value: string;
}

export interface OAuthInput {
  grant: OAuthGrant;
  clientId: string;
  clientSecret?: string | null;
  authorizeUrl?: string | null;
  tokenUrl: string;
  scopes: string[];
  accessToken?: string | null;
  refreshToken?: string | null;
}

export interface AuthInput {
  kind: AuthKind;
  bearerToken?: string | null;
  basicUsername?: string | null;
  basicPassword?: string | null;
  basicRealm?: string | null;
  headers: HeaderPair[];
  oauth?: OAuthInput | null;
}

export type ToolPolicyMode = "allowAll" | "readOnly" | "allowlist" | "denylist";

export interface ToolPolicy {
  mode: ToolPolicyMode;
  allowedTools: string[];
  deniedTools: string[];
}

export interface ServerInput {
  name: string;
  slug: string;
  url: string;
  transport: Transport;
  enabled: boolean;
  auth: AuthInput;
  toolPolicy?: ToolPolicy | null;
  serverId?: string;
}

export interface OAuthPublic {
  grant: OAuthGrant;
  clientId: string;
  authorizeUrl?: string | null;
  tokenUrl: string;
  scopes: string[];
  hasClientSecret: boolean;
  hasAccessToken: boolean;
  hasRefreshToken: boolean;
  expiresAt?: number | null;
}

export interface AuthPublic {
  kind: AuthKind;
  bearerConfigured: boolean;
  basicUsername?: string | null;
  basicRealm?: string | null;
  headerNames: string[];
  oauth?: OAuthPublic | null;
}

export interface ToolDef {
  name: string;
  description?: string;
  inputSchema?: JsonSchema;
  [key: string]: unknown;
}

export interface JsonSchema {
  type?: string | string[];
  description?: string;
  enum?: unknown[];
  default?: unknown;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  items?: JsonSchema;
}

export interface ServerView {
  id: string;
  name: string;
  slug: string;
  url: string;
  transport: Transport;
  enabled: boolean;
  auth: AuthPublic;
  toolPolicy?: ToolPolicy;
  state: ConnState;
  message: string;
  toolCount: number;
  exposedToolCount?: number;
  tools: ToolDef[];
  serverInfo?: Record<string, unknown> | null;
  createdAt: string;
  updatedAt: string;
  checkedAt?: string | null;
  totalCalls?: number;
  errorCount?: number;
  lastLatencyMs?: number | null;
}

export interface Discovery {
  authRequired: boolean;
  authorizationEndpoint?: string | null;
  tokenEndpoint?: string | null;
  registrationEndpoint?: string | null;
  scopes: string[];
  realm?: string | null;
  resource?: string | null;
  message: string;
}

export interface ProbeResult {
  ok: boolean;
  durationMs?: number;
  message: string;
  serverInfo?: Record<string, unknown> | null;
  tools: ToolDef[];
}

export interface ProbeCallResult {
  ok: boolean;
  durationMs: number;
  message: string;
  result?: Record<string, unknown> | null;
}

export interface ActivityEntry {
  seq: number;
  timestamp: string;
  serverId?: string | null;
  slug: string;
  source: "system" | "admin" | "mcp" | "probe" | string;
  kind: "state" | "handshake" | "oauth" | "tool" | string;
  phase: "start" | "ok" | "error" | string;
  method: string;
  tool?: string | null;
  durationMs?: number | null;
  ok: boolean;
  message: string;
  requestPreview?: string | null;
  responsePreview?: string | null;
}

export interface Meta {
  version: string;
  authRequired: boolean;
  publicBaseUrl: string;
  mcpUrl: string;
}

export interface ImportedCandidate {
  input: ServerInput;
  discovery?: Discovery | null;
  notes: string[];
  warnings: string[];
}

export interface ImportResponse {
  detectedFormat: string;
  sourceLabel: string;
  candidates: ImportedCandidate[];
}

export interface RegistryEntry {
  slug: string;
  name: string;
  provider: string;
  category: "productivity" | "developer" | "cloud" | "data" | "design" | string;
  description: string;
  url: string;
  transport: Transport;
  authKind: AuthKind;
  authLabel: string;
  secretHint?: string | null;
  installCommand: string;
  featured: boolean;
}
