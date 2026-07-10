export interface Account {
  id: string;
  name: string;
  username: string;
  password: string;
  key: string;
  enabled: boolean;
  cooldown_until: string | null;
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

export interface AccountInput {
  name: string;
  username?: string;
  password?: string;
  key: string;
}

export interface AccountUpdate {
  name?: string;
  username?: string;
  password?: string;
  key?: string;
  enabled?: boolean;
}

export interface AppConfig {
  gateway_port: number;
  gateway_key: string;
  upstream_base_url: string;
  auto_start: boolean;
  remote: RemoteSync;
}

export interface RemoteSync {
  url: string;
  token: string;
}

export interface GatewayStatus {
  running: boolean;
  port: number;
  key: string;
  upstream_base_url: string;
  last_error: string | null;
}

export interface GatewayLog {
  id: number;
  level: string;
  category: string;
  message: string;
  created_at: string;
}

export interface ForwardLog {
  id: number;
  timestamp: string;
  model: string;
  account_id: string;
  account_name: string;
  status: string;
  http_status: number | null;
  prompt_tokens: number;
  completion_tokens: number;
  cached_tokens: number;
  cost: number;
  error_message: string | null;
}

export interface UsageWindow {
  account_id: string;
  window_5h: number;
  window_week: number;
  window_month: number;
}

export interface DashboardSummary {
  total_accounts: number;
  available_accounts: number;
  gateway_running: boolean;
  today_cost: number;
  week_cost: number;
  month_cost: number;
}

export interface DailyModelCost {
  date: string;
  model: string;
  cost: number;
}

export interface RemoteNodeStatus {
  url: string;
  version: string;
  gateway: {
    running: boolean;
    port: number;
    upstream_base_url: string;
    last_error: string | null;
  };
  accounts: {
    total: number;
    enabled: number;
    disabled: number;
    cooldown: number;
    available: number;
  };
  usage: {
    today_cost: number;
    week_cost: number;
    month_cost: number;
  };
  last_error: string | null;
}

export interface RemoteTestResult {
  ok: boolean;
  message: string;
}

export interface RemoteSyncResult {
  pushed: number;
  message: string;
}

const TOKEN_KEY = "ocg-dashboard-token";

function dashboardToken(): string {
  const url = new URL(window.location.href);
  const token = url.searchParams.get("token");
  if (token) {
    sessionStorage.setItem(TOKEN_KEY, token);
    url.searchParams.delete("token");
    window.history.replaceState({}, "", url);
    return token;
  }
  return sessionStorage.getItem(TOKEN_KEY) || "";
}

function apiBase(): string {
  if (window.location.pathname.startsWith("/dashboard")) {
    return "/dashboard/api";
  }
  return "http://127.0.0.1:9042/dashboard/api";
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = dashboardToken();
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${token}`);
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  const response = await fetch(`${apiBase()}${path}`, { ...init, headers });
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    try {
      const body = await response.json();
      if (body?.error) message = body.error;
    } catch {
      const text = await response.text().catch(() => "");
      if (text) message = text;
    }
    throw new Error(message);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

function jsonBody(value: unknown): BodyInit {
  return JSON.stringify(value);
}

export const tauriApi = {
  getAccounts: () => request<Account[]>("/accounts"),
  createAccount: (input: AccountInput) =>
    request<Account>("/accounts", { method: "POST", body: jsonBody(input) }),
  updateAccount: (id: string, update: AccountUpdate) =>
    request<Account>(`/accounts/${id}`, { method: "PATCH", body: jsonBody(update) }),
  deleteAccount: (id: string) => request<void>(`/accounts/${id}`, { method: "DELETE" }),
  toggleAccount: (id: string) => request<Account>(`/accounts/${id}/toggle`, { method: "POST" }),
  testAccount: async (id: string) => {
    const result = await request<{ message: string }>(`/accounts/${id}/test`, { method: "POST" });
    return result.message;
  },
  getAccountUsage: (id: string) => request<UsageWindow>(`/accounts/${id}/usage`),
  resetAccountCooldown: (id: string) =>
    request<Account>(`/accounts/${id}/reset-cooldown`, { method: "POST" }),

  getSettings: () => request<AppConfig>("/settings"),
  updateSettings: (config: AppConfig) =>
    request<GatewayStatus>("/settings", { method: "POST", body: jsonBody(config) }),
  regenerateGatewayKey: async () => {
    const result = await request<{ key: string }>("/settings/regenerate-gateway-key", {
      method: "POST",
    });
    return result.key;
  },

  getGatewayStatus: () => request<GatewayStatus>("/gateway/status"),

  getGatewayLogs: (limit?: number) => request<GatewayLog[]>(`/logs/gateway?limit=${limit ?? 100}`),
  getForwardLogs: (limit?: number) => request<ForwardLog[]>(`/logs/forward?limit=${limit ?? 100}`),

  getDashboardSummary: () => request<DashboardSummary>("/dashboard/summary"),
  getDailyCostByModel: (days?: number) =>
    request<DailyModelCost[]>(`/dashboard/daily-cost-by-model?days=${days ?? 30}`),

  testRemote: (url: string, token: string) =>
    request<RemoteTestResult>("/remote/test", {
      method: "POST",
      body: jsonBody({ url, token }),
    }),
  getRemoteNodeStatus: () => request<RemoteNodeStatus>("/remote/status"),
  pushLocalToRemote: () =>
    request<RemoteSyncResult>("/remote/push", { method: "POST" }),
};
