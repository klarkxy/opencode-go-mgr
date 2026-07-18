<template>
  <n-tooltip trigger="hover" :disabled="menuShown">
    <template #trigger>
      <n-dropdown
        trigger="click"
        :show="menuShown"
        :options="menuOptions"
        @select="selectLocale"
        @update:show="menuShown = $event"
      >
        <n-button
          circle
          quaternary
          aria-haspopup="menu"
          :aria-expanded="menuShown"
          :aria-label="t('语言：{language}', { language: localeLabel })"
        >
          <template #icon><n-icon :component="GlobalOutlined" /></template>
        </n-button>
      </n-dropdown>
    </template>
    {{ t("语言：{language}", { language: localeLabel }) }}
  </n-tooltip>
</template>

<script setup lang="ts">
import { computed, h, ref } from "vue";
import { NButton, NDropdown, NIcon, NTooltip } from "naive-ui";
import type { DropdownOption } from "naive-ui";
import { CheckOutlined, GlobalOutlined } from "@vicons/antd";
import {
  isLocale,
  locale,
  localeLabel,
  LOCALE_OPTIONS,
  setLocale,
  t,
} from "../i18n/index.ts";

const menuShown = ref(false);

const menuOptions = computed<DropdownOption[]>(() => LOCALE_OPTIONS.map((option) => ({
  key: option.value,
  label: option.label,
  extra: locale.value === option.value
    ? () => h(NIcon, { component: CheckOutlined, size: 14, "aria-hidden": true })
    : undefined,
})));

function selectLocale(key: string | number) {
  if (typeof key === "string" && isLocale(key)) setLocale(key);
}
</script>
