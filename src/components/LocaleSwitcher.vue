<template>
  <span class="locale-switcher">
    <n-button circle quaternary tabindex="-1" aria-hidden="true">
      <template #icon><n-icon :component="GlobalOutlined" /></template>
    </n-button>
    <select
      :value="locale"
      :aria-label="t('语言：{language}', { language: localeLabel })"
      :title="t('语言：{language}', { language: localeLabel })"
      @change="selectLocale"
    >
      <option v-for="option in LOCALE_OPTIONS" :key="option.value" :value="option.value">
        {{ option.label }}
      </option>
    </select>
  </span>
</template>

<script setup lang="ts">
import { NButton, NIcon } from "naive-ui";
import { GlobalOutlined } from "@vicons/antd";
import {
  isLocale,
  locale,
  localeLabel,
  LOCALE_OPTIONS,
  setLocale,
  t,
} from "../i18n/index.ts";

function selectLocale(event: Event) {
  const value = (event.target as HTMLSelectElement).value;
  if (isLocale(value)) setLocale(value);
}
</script>

<style scoped>
.locale-switcher {
  position: relative;
  display: inline-flex;
  border-radius: 50%;
}

.locale-switcher:focus-within {
  outline: 2px solid var(--ocg-primary);
  outline-offset: 2px;
}

select {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  opacity: 0;
  cursor: pointer;
}
</style>
