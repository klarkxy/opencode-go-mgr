/**
 * Dev-mode mock for Tauri `invoke`.
 *
 * When the frontend is opened in a plain browser (e.g. `npm run dev` or
 * Playwright), `window.__TAURI__` is undefined and every backend call fails.
 * This module installs a lightweight mock so that UI development and smoke
 * tests can run without launching the full Tauri app.
 *
 * In the real Tauri app `window.__TAURI__` is already present, so this module
 * does nothing and the real Rust backend is used.
 */

import type {
  Account,
  AccountInput,
  AccountUpdate,
  AppConfig,
  DashboardSummary,
  DailyModelCost,
  ForwardLog,
  GatewayLog,
  GatewayStatus,
  UsageWindow,
} from "./tauri";

const STORAGE_KEY = "ocg-manager-dev-mock";

interface MockState {
  accounts: Account[];
  config: AppConfig;
  gatewayStatus: GatewayStatus;
  forwardLogs: ForwardLog[];
  gatewayLogs: GatewayLog[];
}

function now(): string {
  return new Date().toISOString();
}

function generateId(): string {
  return `dev-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function generateKey(): string {
  const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let key = "ocg-";
  for (let i = 0; i < 32; i++) {
    key += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return key;
}

function loadState(): MockState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      return JSON.parse(raw) as MockState;
    }
  } catch {
    // ignore
  }
  return {
    accounts: [],
    config: {
      gateway_port: 9042,
      gateway_key: generateKey(),
      upstream_base_url: "https://opencode.ai/zen/go/v1",
      auto_start: false,
      remote: { url: "", token: "" },
    },
    gatewayStatus: {
      running: true,
      port: 9042,
      key: generateKey(),
      upstream_base_url: "https://opencode.ai/zen/go/v1",
    },
    forwardLogs: [],
    gatewayLogs: [
      {
        id: 1,
        level: "INFO",
        category: "gateway",
        message: "Dev mock gateway started",
        created_at: now(),
      },
    ],
  };
}

function saveState(state: MockState) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // ignore
  }
}

function maskKey(key: string): string {
  if (key.length <= 8) return key;
  return `${key.slice(0, 4)}...${key.slice(-4)}`;
}

function computeUsage(_accountId: string): UsageWindow {
  return {
    account_id: _accountId,
    window_5h: 0,
    window_week: 0,
    window_month: 0,
  };
}

// Stable-model set for the dev-mock chart. Mirrors the model names ocg-core
// actually sees from the upstream (chat-variant + reasoning + mini), so the
// legend colors line up with what a real dataset would show.
const MOCK_MODELS = ["claude-3-7-sonnet", "claude-3-5-haiku", "gpt-4o-mini", "claude-opus-4"];

/// Generate a deterministic-ish 30-day dataset of per-day, per-model cost so
/// the dashboard chart is never empty in browser dev / Playwright runs. The
/// shape is a gentle decay with weekly wobble + two spike days, so the chart
/// reads like a real usage curve, not random noise.
function generateMockDailyCosts(days = 30): DailyModelCost[] {
  const out: DailyModelCost[] = [];
  const today = new Date();
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(today);
    d.setUTCDate(today.getUTCDate() - i);
    const date = d.toISOString().slice(0, 10);
    // base decay: older days less, recent days more; plus a weekly bump
    const recent = (days - i) / days; // 0 → 1
    const weeklyBump = [0.4, 0.2, 0.1, 0.0, 0.0, 0.6, 0.9][d.getUTCDay()];
    // two spike days to make the chart visually interesting
    const spike = i === 3 || i === 9 ? 1.8 : 1.0;
    const base = (0.05 + recent * 0.55) * (0.6 + weeklyBump) * spike;
    // distribute base across models with fixed-ish weights
    const weights = [0.5, 0.25, 0.18, 0.07];
    MOCK_MODELS.forEach((model, idx) => {
      const cost = Math.max(0, +(base * weights[idx] * (0.8 + Math.random() * 0.4)).toFixed(4));
      if (cost > 0.0001) {
        out.push({ date, model, cost });
      }
    });
  }
  return out;
}

let cachedMockDailyCosts: DailyModelCost[] | null = null;

function dashboardSummary(state: MockState): DashboardSummary {
  const costs = cachedMockDailyCosts ?? (cachedMockDailyCosts = generateMockDailyCosts());
  const now = new Date();
  const todayStr = now.toISOString().slice(0, 10);
  const weekAgo = new Date(now);
  weekAgo.setUTCDate(now.getUTCDate() - 7);
  const weekStart = weekAgo.toISOString().slice(0, 10);
  const monthAgo = new Date(now);
  monthAgo.setUTCDate(now.getUTCDate() - 30);
  const monthStart = monthAgo.toISOString().slice(0, 10);
  const today_cost = costs
    .filter((c) => c.date === todayStr)
    .reduce((s, c) => s + c.cost, 0);
  const week_cost = costs
    .filter((c) => c.date >= weekStart)
    .reduce((s, c) => s + c.cost, 0);
  const month_cost = costs
    .filter((c) => c.date >= monthStart)
    .reduce((s, c) => s + c.cost, 0);
  return {
    total_accounts: state.accounts.length,
    available_accounts: state.accounts.filter((a) => a.enabled).length,
    gateway_running: state.gatewayStatus.running,
    today_cost: +today_cost.toFixed(4),
    week_cost: +week_cost.toFixed(4),
    month_cost: +month_cost.toFixed(4),
  };
}

const handlers: Record<string, (args: Record<string, unknown>, state: MockState) => unknown> = {
  get_accounts: (_args, state) => state.accounts,

  create_account: (args, state) => {
    const input = args.input as AccountInput;
    const account: Account = {
      id: generateId(),
      name: input.name,
      key_cipher: `cipher:${maskKey(input.key)}`,
      enabled: true,
      referral_code: input.referral_code || null,
      recharge_date: input.recharge_date || null,
      cooldown_until: null,
      last_error: null,
      created_at: now(),
      updated_at: now(),
    };
    state.accounts.push(account);
    state.gatewayLogs.unshift({
      id: state.gatewayLogs.length + 1,
      level: "INFO",
      category: "account",
      message: `Account created: ${account.name}`,
      created_at: now(),
    });
    saveState(state);
    return account;
  },

  update_account: (args, state) => {
    const id = args.id as string;
    const update = args.update as AccountUpdate;
    const account = state.accounts.find((a) => a.id === id);
    if (!account) throw new Error(`Account not found: ${id}`);
    if (update.name !== undefined) account.name = update.name;
    if (update.key !== undefined) account.key_cipher = `cipher:${maskKey(update.key)}`;
    if (update.enabled !== undefined) account.enabled = update.enabled;
    if (update.referral_code !== undefined) account.referral_code = update.referral_code || null;
    if (update.recharge_date !== undefined) account.recharge_date = update.recharge_date || null;
    account.updated_at = now();
    saveState(state);
    return account;
  },

  delete_account: (args, state) => {
    const id = args.id as string;
    state.accounts = state.accounts.filter((a) => a.id !== id);
    saveState(state);
    return null;
  },

  toggle_account: (args, state) => {
    const id = args.id as string;
    const account = state.accounts.find((a) => a.id === id);
    if (!account) throw new Error(`Account not found: ${id}`);
    account.enabled = !account.enabled;
    account.updated_at = now();
    saveState(state);
    return account;
  },

  test_account: (args, state) => {
    const id = args.id as string;
    const account = state.accounts.find((a) => a.id === id);
    if (!account) throw new Error(`Account not found: ${id}`);
    return `Dev mock test passed for ${account.name}: ${maskKey(account.key_cipher.replace("cipher:", ""))}`;
  },

  reset_account_cooldown: (args, state) => {
    const id = args.id as string;
    const account = state.accounts.find((a) => a.id === id);
    if (!account) throw new Error(`Account not found: ${id}`);
    account.cooldown_until = null;
    account.last_error = null;
    account.updated_at = now();
    saveState(state);
    return account;
  },

  get_account_usage: (args, _state) => {
    const id = args.id as string;
    return computeUsage(id);
  },

  get_settings: (_args, state) => state.config,

  update_settings: (args, state) => {
    const config = args.config as AppConfig;
    state.config = { ...config };
    state.gatewayStatus.port = config.gateway_port;
    state.gatewayStatus.key = config.gateway_key;
    state.gatewayStatus.upstream_base_url = config.upstream_base_url;
    saveState(state);
    return state.gatewayStatus;
  },

  regenerate_gateway_key: (_args, state) => {
    const key = generateKey();
    state.config.gateway_key = key;
    state.gatewayStatus.key = key;
    saveState(state);
    return key;
  },

  get_gateway_status: (_args, state) => state.gatewayStatus,

  restart_gateway: (_args, state) => {
    state.gatewayStatus.running = true;
    state.gatewayLogs.unshift({
      id: state.gatewayLogs.length + 1,
      level: "INFO",
      category: "gateway",
      message: "Gateway restarted",
      created_at: now(),
    });
    saveState(state);
    return state.gatewayStatus;
  },

  get_gateway_logs: (args, state) => {
    const limit = (args.limit as number) ?? 100;
    return state.gatewayLogs.slice(0, limit);
  },

  get_forward_logs: (_args, state) => state.forwardLogs,

  open_browser: (args, state) => {
    const accountId = args.accountId as string;
    const account = state.accounts.find((a) => a.id === accountId);
    if (!account) throw new Error(`Account not found: ${accountId}`);
    return `https://platform.opencode.ai/account?mock=${account.id}`;
  },

  close_browser: () => null,

  get_dashboard_summary: (_args, state) => dashboardSummary(state),

  get_daily_cost_by_model: (args) => {
    const days = (args.days as number) ?? 30;
    if (!cachedMockDailyCosts) {
      cachedMockDailyCosts = generateMockDailyCosts(30);
    }
    if (days === 30) return cachedMockDailyCosts;
    // for non-default ranges, regenerate lazily
    return generateMockDailyCosts(days);
  },
};

export function installDevMock() {
  if (typeof window === "undefined") return;
  if ((window as unknown as Record<string, unknown>).__TAURI_INTERNALS__) return;

  const state = loadState();

  ((window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ as unknown) = {
    invoke: async (cmd: string, args?: Record<string, unknown>) => {
      const handler = handlers[cmd];
      if (!handler) {
        throw new Error(`Dev mock: unknown command "${cmd}"`);
      }
      // simulate async latency
      await new Promise((resolve) => setTimeout(resolve, 30));
      return handler(args ?? {}, state);
    },
  };

  console.log("[ocg-manager] Dev mock installed — backend calls are simulated.");
}

installDevMock();
