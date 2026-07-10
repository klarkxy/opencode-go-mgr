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
  today_cost: number;
  week_cost: number;
  month_cost: number;
}

export interface DailyModelCost {
  date: string;
  model: string;
  cost: number;
}

export const DASHBOARD_AUTH_REQUIRED_EVENT = "ocg-dashboard-auth-required";

export interface DashboardAuthStatus {
  local: boolean;
  initialized: boolean;
  authenticated: boolean;
}

export class DashboardAuthError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DashboardAuthError";
  }
}

function dashboardAuthError(message: string): DashboardAuthError {
  const error = new DashboardAuthError(message);
  window.dispatchEvent(new CustomEvent(DASHBOARD_AUTH_REQUIRED_EVENT, { detail: message }));
  return error;
}

function apiBase(): string {
  if (window.location.pathname.startsWith("/dashboard")) {
    return "/dashboard/api";
  }
  return "http://127.0.0.1:9042/dashboard/api";
}

async function request<T>(
  path: string,
  init: RequestInit = {},
  notifyAuthRequired = true,
): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  const response = await fetch(`${apiBase()}${path}`, {
    ...init,
    headers,
    credentials: "same-origin",
  });
  if (!response.ok) {
    if (response.status === 401 && notifyAuthRequired) {
      throw dashboardAuthError("登录已失效，请重新登录");
    }
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
  getAuthStatus: () => request<DashboardAuthStatus>("/auth/status", {}, false),
  registerAdmin: (username: string, password: string) =>
    request<{ ok: boolean }>(
      "/auth/register",
      { method: "POST", body: jsonBody({ username, password }) },
      false,
    ),
  loginAdmin: (username: string, password: string) =>
    request<{ ok: boolean }>(
      "/auth/login",
      { method: "POST", body: jsonBody({ username, password }) },
      false,
    ),
  logoutAdmin: () =>
    request<void>("/auth/logout", { method: "POST" }, false),

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
  updateSettings: async (config: AppConfig) => {
    await request<unknown>("/settings", { method: "POST", body: jsonBody(config) });
  },
  regenerateGatewayKey: async () => {
    const result = await request<{ key: string }>("/settings/regenerate-gateway-key", {
      method: "POST",
    });
    return result.key;
  },
  getGatewayLogs: (limit?: number) => request<GatewayLog[]>(`/logs/gateway?limit=${limit ?? 100}`),
  getForwardLogs: (limit?: number) => request<ForwardLog[]>(`/logs/forward?limit=${limit ?? 100}`),

  getDashboardSummary: () => request<DashboardSummary>("/dashboard/summary"),
  getDailyCostByModel: (days?: number) =>
    request<DailyModelCost[]>(`/dashboard/daily-cost-by-model?days=${days ?? 30}`),
};
