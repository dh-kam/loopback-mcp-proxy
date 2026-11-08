const TOKEN_KEY = "loopback-admin-token";

export function loadToken(): string {
  return sessionStorage.getItem(TOKEN_KEY) ?? "";
}

export function saveToken(token: string) {
  if (token) sessionStorage.setItem(TOKEN_KEY, token);
  else sessionStorage.removeItem(TOKEN_KEY);
}

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body && !headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  const token = loadToken();
  if (token) headers.set("Authorization", `Bearer ${token}`);
  const response = await fetch(path, { ...init, headers });
  if (response.status === 204) return undefined as T;
  const text = await response.text();
  let payload: unknown = null;
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch {
      payload = text;
    }
  }
  if (!response.ok) {
    const message =
      payload && typeof payload === "object" && payload && "error" in payload
        ? String((payload as { error: unknown }).error)
        : text || response.statusText;
    if (response.status === 401) window.dispatchEvent(new Event("admin-unauthorized"));
    throw new ApiError(response.status, message);
  }
  return payload as T;
}

export function wsUrl(): string {
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  const token = loadToken();
  const query = token ? `?token=${encodeURIComponent(token)}` : "";
  return `${protocol}//${location.host}/api/ws${query}`;
}
