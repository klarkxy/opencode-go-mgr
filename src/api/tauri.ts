import { invoke } from "@tauri-apps/api/core";

export interface Account {
  id: string;
  name: string;
  key_cipher: string;
  enabled: boolean;
  referral_code: string | null;
  recharge_date: string | null;
  cooldown_until: string | null; // RFC3339; null = available
  last_error: string | null;
  created_at: string;
  updated_at: string;
}

export interface AccountInput {
  name: string;
  key: string;
  referral_code?: string;
  recharge_date?: string;
}

export interface AccountUpdate {
  name?: string;
  key?: string;
  enabled?: boolean;
  referral_code?: string;
  recharge_date?: string;
}

export type SelectionStrategy = "sequential" | "random" | "round_robin";

export interface AppConfig {
  gateway_port: number;
  gateway_key: string;
  selection_strategy: SelectionStrategy;
  upstream_base_url: string;
  auto_start: boolean;
  remote: RemoteSync;
}

export interface RemoteSync {
  url: string;
  token: string;
}

export interface RemoteStatus {
  url: string;
  bootstrapped: boolean;
}

export interface RemoteTestResult {
  ok: boolean;
  message: string;
}

export interface GatewayStatus {
  running: boolean;
  port: number;
  key: string;
  upstream_base_url: string;
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
  date: string; // YYYY-MM-DD
  model: string;
  cost: number;
}

export const tauriApi = {
  // Accounts
  getAccounts: () => invoke<Account[]>("get_accounts"),
  createAccount: (input: AccountInput) => invoke<Account>("create_account", { input }),
  updateAccount: (id: string, update: AccountUpdate) =>
    invoke<Account>("update_account", { id, update }),
  deleteAccount: (id: string) => invoke<void>("delete_account", { id }),
  toggleAccount: (id: string) => invoke<Account>("toggle_account", { id }),
  testAccount: (id: string) => invoke<string>("test_account", { id }),
  getAccountUsage: (id: string) => invoke<UsageWindow>("get_account_usage", { id }),
  resetAccountCooldown: (id: string) => invoke<Account>("reset_account_cooldown", { id }),

  // Settings
  getSettings: () => invoke<AppConfig>("get_settings"),
  updateSettings: (config: AppConfig) => invoke<GatewayStatus>("update_settings", { config }),
  regenerateGatewayKey: () => invoke<string>("regenerate_gateway_key"),
  getRemoteStatus: () => invoke<RemoteStatus>("get_remote_status"),
  testRemote: (url: string, token: string) =>
    invoke<RemoteTestResult>("test_remote", { url, token }),

  // Gateway
  getGatewayStatus: () => invoke<GatewayStatus>("get_gateway_status"),
  restartGateway: () => invoke<GatewayStatus>("restart_gateway"),

  // Logs
  getGatewayLogs: (limit?: number) => invoke<GatewayLog[]>("get_gateway_logs", { limit }),
  getForwardLogs: (limit?: number) => invoke<ForwardLog[]>("get_forward_logs", { limit }),

  // Browser
  openBrowser: (accountId: string) => invoke<string>("open_browser", { accountId }),
  closeBrowser: () => invoke<void>("close_browser"),

  // Dashboard
  getDashboardSummary: () => invoke<DashboardSummary>("get_dashboard_summary"),
  getDailyCostByModel: (days?: number) =>
    invoke<DailyModelCost[]>("get_daily_cost_by_model", { days }),
};
