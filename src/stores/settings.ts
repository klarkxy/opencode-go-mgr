import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { dashboardApi, isRevisionConflict } from "../api/dashboard.ts";
import type {
  AppConfig,
  ClaudeDesktopModels,
} from "../api/dashboard.ts";

/**
 * Application settings plus the Claude Desktop three-role mapping.
 *
 * The Settings resource never carries Key plaintext (that lives in the
 * connection store). Update-check / install progress is transient and stays
 * page-local in the Settings view.
 */
export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<AppConfig | null>(null);
  const claudeDesktop = ref<ClaudeDesktopModels | null>(null);
  const loading = ref(false);
  const error = ref("");

  async function load(): Promise<AppConfig> {
    loading.value = true;
    try {
      const result = await dashboardApi.getSettings();
      settings.value = result;
      error.value = "";
      return result;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function loadPresented(): Promise<AppConfig> {
    return load();
  }

  async function putPresented(update: AppConfig): Promise<AppConfig> {
    try {
      await dashboardApi.updateSettings(update);
      await load();
    } catch (cause) {
      if (isRevisionConflict(cause)) await load();
      throw cause;
    }
    if (!settings.value) throw new Error("settings reload returned no resource");
    return settings.value;
  }

  async function loadClaudeDesktop(): Promise<ClaudeDesktopModels> {
    const result = await dashboardApi.getClaudeDesktopModels();
    claudeDesktop.value = result;
    return result;
  }

  async function putClaudeDesktop(models: ClaudeDesktopModels): Promise<ClaudeDesktopModels> {
    try {
      const result = await dashboardApi.updateClaudeDesktopModels(models);
      claudeDesktop.value = result;
      return result;
    } catch (cause) {
      if (isRevisionConflict(cause)) await loadClaudeDesktop();
      throw cause;
    }
  }

  return {
    settings: computed(() => settings.value),
    claudeDesktop: computed(() => claudeDesktop.value),
    loading: computed(() => loading.value),
    error: computed(() => error.value),
    loadPresented,
    putPresented,
    loadClaudeDesktop,
    putClaudeDesktop,
  };
});
