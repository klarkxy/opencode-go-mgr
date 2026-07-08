<template>
  <n-space vertical :size="16">
    <n-space justify="space-between" align="center">
      <n-h3 style="margin: 0">账号管理</n-h3>
      <n-button type="primary" @click="openAddModal">添加账号</n-button>
    </n-space>

    <n-empty v-if="accounts.length === 0" description="暂无账号" />

    <n-card v-for="account in accounts" :key="account.id" :title="account.name" size="small">
      <n-space vertical :size="12">
        <n-space align="center">
          <n-switch :value="account.enabled" @update:value="toggleAccount(account.id)">
            <template #checked>已启用</template>
            <template #unchecked>已禁用</template>
          </n-switch>
          <n-tag v-if="account.referral_code" size="small">邀请码: {{ account.referral_code }}</n-tag>
          <n-tag v-if="account.recharge_date" size="small">充值日: {{ account.recharge_date }}</n-tag>
          <n-tag v-if="isCooling(account)" type="error" size="small">
            冷却中 · 剩 {{ formatRemaining(account) }}
            <template v-if="account.last_error" #tooltip>
              {{ account.last_error }}
            </template>
          </n-tag>
        </n-space>

        <n-space>
          <n-button size="small" @click="openEditModal(account)">编辑</n-button>
          <n-button size="small" @click="testAccount(account.id)">测试</n-button>
          <n-button size="small" @click="openBrowser(account.id)">打开浏览器</n-button>
          <n-button
            v-if="isCooling(account)"
            size="small"
            type="warning"
            @click="resetCooldown(account.id)"
          >
            重置冷却
          </n-button>
          <n-popconfirm @positive-click="deleteAccount(account.id)">
            <template #trigger>
              <n-button size="small" type="error">删除</n-button>
            </template>
            确定删除账号 {{ account.name }} 吗？
          </n-popconfirm>
        </n-space>

        <n-descriptions bordered size="small" :column="3">
          <n-descriptions-item label="5h 用量">
            ${{ getUsage(account.id).window_5h.toFixed(3) }}
          </n-descriptions-item>
          <n-descriptions-item label="本周用量">
            ${{ getUsage(account.id).window_week.toFixed(3) }}
          </n-descriptions-item>
          <n-descriptions-item label="本月用量">
            ${{ getUsage(account.id).window_month.toFixed(3) }}
          </n-descriptions-item>
        </n-descriptions>
      </n-space>
    </n-card>

    <n-modal v-model:show="showModal" :title="modalTitle" preset="card" style="width: 500px">
      <n-form :model="form" label-placement="left" label-width="80">
        <n-form-item label="名称">
          <n-input v-model:value="form.name" placeholder="例如：主号" />
        </n-form-item>
        <n-form-item label="API Key">
          <n-input v-model:value="form.key" type="password" show-password-on="click" placeholder="留空则保持不变" />
        </n-form-item>
        <n-form-item label="邀请码">
          <n-input v-model:value="form.referral_code" placeholder="可选" />
        </n-form-item>
        <n-form-item label="充值日">
          <n-input v-model:value="form.recharge_date" placeholder="可选，例如 2026-07-01" />
        </n-form-item>
      </n-form>
      <template #footer>
        <n-space justify="end">
          <n-button @click="showModal = false">取消</n-button>
          <n-button type="primary" @click="submitForm">保存</n-button>
        </n-space>
      </template>
    </n-modal>
  </n-space>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import {
  NSpace,
  NH3,
  NButton,
  NCard,
  NSwitch,
  NTag,
  NDescriptions,
  NDescriptionsItem,
  NEmpty,
  NModal,
  NForm,
  NFormItem,
  NInput,
  NPopconfirm,
  useMessage,
} from "naive-ui";
import { tauriApi, Account, AccountInput, AccountUpdate, UsageWindow } from "../api/tauri";

const message = useMessage();
const accounts = ref<Account[]>([]);
const usageMap = ref<Record<string, UsageWindow>>({});
const showModal = ref(false);
const editingId = ref<string | null>(null);
const form = ref<AccountInput>({
  name: "",
  key: "",
  referral_code: "",
  recharge_date: "",
});

const modalTitle = computed(() => (editingId.value ? "编辑账号" : "添加账号"));

function getUsage(accountId: string): UsageWindow {
  return (
    usageMap.value[accountId] || {
      account_id: accountId,
      window_5h: 0,
      window_week: 0,
      window_month: 0,
    }
  );
}

function isCooling(account: Account): boolean {
  if (!account.cooldown_until) return false;
  return new Date(account.cooldown_until).getTime() > Date.now();
}

function formatRemaining(account: Account): string {
  if (!account.cooldown_until) return "";
  const ms = new Date(account.cooldown_until).getTime() - Date.now();
  if (ms <= 0) return "0s";
  const min = Math.floor(ms / 60000);
  if (min < 60) return `${min}min`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h${min % 60}m`;
  const day = Math.floor(hr / 24);
  return `${day}天${hr % 24}h`;
}

async function resetCooldown(id: string) {
  try {
    await tauriApi.resetAccountCooldown(id);
    message.success("已重置冷却");
    await loadAccounts();
  } catch (e) {
    message.error(`重置失败: ${e}`);
  }
}

async function loadAccounts() {
  try {
    accounts.value = await tauriApi.getAccounts();
    usageMap.value = {};
    for (const account of accounts.value) {
      usageMap.value[account.id] = await tauriApi.getAccountUsage(account.id);
    }
  } catch (e) {
    message.error(`加载账号失败: ${e}`);
  }
}

function openAddModal() {
  editingId.value = null;
  form.value = { name: "", key: "", referral_code: "", recharge_date: "" };
  showModal.value = true;
}

function openEditModal(account: Account) {
  editingId.value = account.id;
  form.value = {
    name: account.name,
    key: "",
    referral_code: account.referral_code || "",
    recharge_date: account.recharge_date || "",
  };
  showModal.value = true;
}

async function submitForm() {
  if (!form.value.name) {
    message.warning("请填写名称");
    return;
  }
  try {
    if (editingId.value) {
      const update: AccountUpdate = {
        name: form.value.name,
        referral_code: form.value.referral_code,
        recharge_date: form.value.recharge_date,
      };
      if (form.value.key) {
        update.key = form.value.key;
      }
      await tauriApi.updateAccount(editingId.value, update);
      message.success("账号已更新");
    } else {
      if (!form.value.key) {
        message.warning("请填写 API Key");
        return;
      }
      await tauriApi.createAccount(form.value);
      message.success("账号已添加");
    }
    form.value = { name: "", key: "", referral_code: "", recharge_date: "" };
    showModal.value = false;
    await loadAccounts();
  } catch (e) {
    message.error(`保存失败: ${e}`);
  }
}

async function toggleAccount(id: string) {
  try {
    await tauriApi.toggleAccount(id);
    await loadAccounts();
  } catch (e) {
    message.error(`切换失败: ${e}`);
  }
}

async function deleteAccount(id: string) {
  try {
    await tauriApi.deleteAccount(id);
    message.success("账号已删除");
    await loadAccounts();
  } catch (e) {
    message.error(`删除失败: ${e}`);
  }
}

async function testAccount(id: string) {
  try {
    const result = await tauriApi.testAccount(id);
    message.info(result);
  } catch (e) {
    message.error(`测试失败: ${e}`);
  }
}

async function openBrowser(id: string) {
  try {
    await tauriApi.openBrowser(id);
    message.success("已打开浏览器窗口");
  } catch (e) {
    message.error(`打开浏览器失败: ${e}`);
  }
}

onMounted(loadAccounts);
</script>
