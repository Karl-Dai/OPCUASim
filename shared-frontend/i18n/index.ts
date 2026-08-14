import { ref, computed } from 'vue'
import type { Locale } from './types'
import { SUPPORTED_LOCALES, STORAGE_KEY } from './types'
import { detectSystemLocale } from './detect'
import zhCN from '@app/i18n/locales/zh-CN'
import enUS from '@app/i18n/locales/en-US'
import type { DictShape } from '@app/i18n/locales/zh-CN'

const dictionaries: Record<Locale, DictShape> = {
  'zh-CN': zhCN,
  'en-US': enUS,
}

function initialLocale(): Locale {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (saved && (SUPPORTED_LOCALES as readonly string[]).includes(saved)) {
      return saved as Locale
    }
  } catch { /* ignore */ }
  return detectSystemLocale()
}

export const locale = ref<Locale>(initialLocale())

function lookup(dict: DictShape, key: string): string | undefined {
  const parts = key.split('.')
  let cur: unknown = dict
  for (const p of parts) {
    if (cur && typeof cur === 'object' && p in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[p]
    } else {
      return undefined
    }
  }
  return typeof cur === 'string' ? cur : undefined
}

function interpolate(template: string, params?: Record<string, unknown>): string {
  if (!params) return template
  return template.replace(/\{(\w+)\}/g, (_, name) => {
    const v = params[name]
    return v === undefined || v === null ? `{${name}}` : String(v)
  })
}

function translate(key: string, params?: Record<string, unknown>): string {
  const fromCurrent = lookup(dictionaries[locale.value], key)
  if (fromCurrent !== undefined) return interpolate(fromCurrent, params)
  const fromFallback = lookup(dictionaries['en-US'], key)
  if (fromFallback !== undefined) return interpolate(fromFallback, params)
  return key
}

function setLocale(next: Locale) {
  if (!(SUPPORTED_LOCALES as readonly string[]).includes(next)) return
  locale.value = next
  try { localStorage.setItem(STORAGE_KEY, next) } catch { /* ignore */ }
}

const localeRef = computed(() => locale.value)

export function useI18n() {
  return { t: translate, locale: localeRef, setLocale }
}

// Test-only: re-runs initial locale detection. Don't use in production code.
export function __resetForTests() {
  locale.value = initialLocale()
}

export type { Locale }

// Category identifiers returned by current backends, plus legacy Chinese
// labels retained for configs/snapshots created before stable keys existed.
const CATEGORY_LABEL_TO_KEY: Record<string, string> = {
  single_point: 'single_point',
  double_point: 'double_point',
  step_position: 'step_position',
  bitstring: 'bitstring',
  normalized_measured: 'normalized_measured',
  scaled_measured: 'scaled_measured',
  float_measured: 'float_measured',
  integrated_totals: 'integrated_totals',
  single_command: 'single_command',
  double_command: 'double_command',
  step_command: 'step_command',
  bitstring_command: 'bitstring_command',
  normalized_setpoint: 'normalized_setpoint',
  scaled_setpoint: 'scaled_setpoint',
  float_setpoint: 'float_setpoint',
  '单点 (SP)': 'single_point',
  '双点 (DP)': 'double_point',
  '步位置 (ST)': 'step_position',
  '位串 (BO)': 'bitstring',
  '归一化 (ME_NA)': 'normalized_measured',
  '标度化 (ME_NB)': 'scaled_measured',
  '浮点 (ME_NC)': 'float_measured',
  '累计量 (IT)': 'integrated_totals',
}

export function localizeCategoryLabel(label: string): string {
  const key = CATEGORY_LABEL_TO_KEY[label]
  return key ? translate(`category.${key}`) : label
}
