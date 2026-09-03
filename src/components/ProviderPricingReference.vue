<template>
  <div class="provider-pricing-reference">
    <OllamaPricingReference
      v-if="kind === 'ollama'"
      :snapshot="snapshot"
    />
    <GoatQuotaReference
      v-else
      :snapshot="snapshot"
      :saving-model-id="savingModelId"
      :disabled="disabled"
      @save-multiplier="(modelId, multiplier) => emit('save-multiplier', modelId, multiplier)"
    />
  </div>
</template>

<script setup lang="ts">
import GoatQuotaReference from "./GoatQuotaReference.vue";
import OllamaPricingReference from "./OllamaPricingReference.vue";
import type { ProviderNeutralPricingSnapshot } from "../api/providers.ts";

withDefaults(defineProps<{
  kind?: "goat" | "ollama";
  snapshot?: ProviderNeutralPricingSnapshot | null;
  savingModelId?: string | null;
  disabled?: boolean;
}>(), {
  kind: "goat",
});

const emit = defineEmits<{
  "save-multiplier": [modelId: string, multiplier: number];
}>();
</script>

<style scoped>
.provider-pricing-reference {
  min-width: 0;
}
</style>
