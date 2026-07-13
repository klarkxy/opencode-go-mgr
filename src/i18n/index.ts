import { computed, ref, watch } from "vue";
import * as naiveUiModule from "naive-ui";

const cjsNaiveUi = naiveUiModule as unknown as { default: typeof naiveUiModule };
const naiveUi = "dateDeDE" in (naiveUiModule as object) ? naiveUiModule : cjsNaiveUi.default;
const {
  dateDeDE,
  dateEnUS,
  dateEsAR,
  dateFrFR,
  dateJaJP,
  dateKoKR,
  datePtBR,
  dateRuRU,
  dateZhCN,
  dateZhTW,
  deDE,
  enUS,
  esAR,
  frFR,
  jaJP,
  koKR,
  ptBR,
  ruRU,
  zhCN,
  zhTW,
} = naiveUi;
import { deDEMessages } from "./messages/de-DE.ts";
import { enUSMessages, type MessageKey, type Messages } from "./messages/en-US.ts";
import { esESMessages } from "./messages/es-ES.ts";
import { frFRMessages } from "./messages/fr-FR.ts";
import { jaJPMessages } from "./messages/ja-JP.ts";
import { koKRMessages } from "./messages/ko-KR.ts";
import { ptBRMessages } from "./messages/pt-BR.ts";
import { ruRUMessages } from "./messages/ru-RU.ts";
import { zhTWMessages } from "./messages/zh-TW.ts";

export type { MessageKey, Messages } from "./messages/en-US.ts";

export const LOCALE_STORAGE_KEY = "ocg-manager.locale";
export const DEFAULT_LOCALE = "zh-CN";
export const LOCALE_OPTIONS = [
  { value: "zh-CN", label: "简体中文" },
  { value: "zh-TW", label: "繁體中文" },
  { value: "en-US", label: "English" },
  { value: "ja-JP", label: "日本語" },
  { value: "ko-KR", label: "한국어" },
  { value: "es-ES", label: "Español" },
  { value: "fr-FR", label: "Français" },
  { value: "de-DE", label: "Deutsch" },
  { value: "pt-BR", label: "Português (Brasil)" },
  { value: "ru-RU", label: "Русский" },
] as const;

export type Locale = (typeof LOCALE_OPTIONS)[number]["value"];
export type TranslationParams = Record<string, string | number>;

const localeValues = new Set<string>(LOCALE_OPTIONS.map(({ value }) => value));
const zhCNMessages = Object.fromEntries(
  Object.keys(enUSMessages).map((key) => [key, key]),
) as Messages;

export const messages: Record<Locale, Messages> = {
  "zh-CN": zhCNMessages,
  "zh-TW": zhTWMessages,
  "en-US": enUSMessages,
  "ja-JP": jaJPMessages,
  "ko-KR": koKRMessages,
  "es-ES": esESMessages,
  "fr-FR": frFRMessages,
  "de-DE": deDEMessages,
  "pt-BR": ptBRMessages,
  "ru-RU": ruRUMessages,
};

const naiveLocales = {
  "zh-CN": { locale: zhCN, dateLocale: dateZhCN },
  "zh-TW": { locale: zhTW, dateLocale: dateZhTW },
  "en-US": { locale: enUS, dateLocale: dateEnUS },
  "ja-JP": { locale: jaJP, dateLocale: dateJaJP },
  "ko-KR": { locale: koKR, dateLocale: dateKoKR },
  "es-ES": { locale: esAR, dateLocale: dateEsAR },
  "fr-FR": { locale: frFR, dateLocale: dateFrFR },
  "de-DE": { locale: deDE, dateLocale: dateDeDE },
  "pt-BR": { locale: ptBR, dateLocale: datePtBR },
  "ru-RU": { locale: ruRU, dateLocale: dateRuRU },
} as const;

export function isLocale(value: string | null | undefined): value is Locale {
  return typeof value === "string" && localeValues.has(value);
}

export function matchLocale(value: string | null | undefined): Locale | null {
  if (!value) return null;
  const normalized = value.replaceAll("_", "-").toLowerCase();
  const exact = LOCALE_OPTIONS.find(({ value: option }) => option.toLowerCase() === normalized);
  if (exact) return exact.value;
  const [language] = normalized.split("-");
  if (language === "zh") {
    return /(?:^|-)hant(?:-|$)|-(?:tw|hk|mo)(?:-|$)/.test(normalized) ? "zh-TW" : "zh-CN";
  }
  return LOCALE_OPTIONS.find(({ value: option }) => option.toLowerCase().startsWith(`${language}-`))?.value ?? null;
}

export function resolveLocale(
  stored: string | null | undefined,
  preferred: readonly string[] = [],
): Locale {
  return matchLocale(stored)
    ?? preferred.map(matchLocale).find((value): value is Locale => value !== null)
    ?? DEFAULT_LOCALE;
}

export function getLocaleStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function readLocale(
  storage: Pick<Storage, "getItem"> | null,
  preferred: readonly string[] = [],
): Locale {
  try {
    return resolveLocale(storage?.getItem(LOCALE_STORAGE_KEY), preferred);
  } catch {
    return resolveLocale(null, preferred);
  }
}

export function writeLocale(storage: Pick<Storage, "setItem"> | null, value: Locale): void {
  try {
    storage?.setItem(LOCALE_STORAGE_KEY, value);
  } catch {
    // A private or locked-down browser may reject persistence; the in-memory locale still works.
  }
}

function browserLocales(): readonly string[] {
  if (typeof window === "undefined" || typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : navigator.language ? [navigator.language] : [];
}

const localeStorage = getLocaleStorage();
export const locale = ref<Locale>(readLocale(localeStorage, browserLocales()));
export const localeLabel = computed(() => (
  LOCALE_OPTIONS.find(({ value }) => value === locale.value)?.label ?? locale.value
));
export const naiveLocale = computed(() => naiveLocales[locale.value].locale);
export const naiveDateLocale = computed(() => naiveLocales[locale.value].dateLocale);

export function setLocale(value: Locale): void {
  locale.value = value;
  writeLocale(localeStorage, value);
}

export function t(key: MessageKey, params: TranslationParams = {}): string {
  return messages[locale.value][key].replace(/\{(\w+)\}/g, (placeholder, name: string) => (
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : placeholder
  ));
}

watch(locale, (value) => {
  if (typeof document !== "undefined") document.documentElement.lang = value;
}, { immediate: true });
