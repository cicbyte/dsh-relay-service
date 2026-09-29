import { defineStore } from 'pinia'

/** 外观模式 */
export type ThemeMode = 'light' | 'dark' | 'auto'

/** 预设主题色（色板顺序即展示顺序） */
export const PRESET_COLORS = [
  { name: '拂晓蓝', value: '#1677ff' },
  { name: '极客蓝', value: '#2f54eb' },
  { name: '酱紫', value: '#722ed1' },
  { name: '品红', value: '#eb2f96' },
  { name: '薄暮红', value: '#f5222f' },
  { name: '日暮橙', value: '#fa8c16' },
  { name: '金盏', value: '#faad14' },
  { name: '极客绿', value: '#52c41a' },
  { name: '明青', value: '#13c2c2' },
]

/** 圆角档位（px，与 AntDV borderRadius token 同步） */
export const RADIUS_OPTIONS = [2, 4, 6, 10]

export interface AppearanceState {
  themeMode: ThemeMode
  primaryColor: string
  radius: number
  compact: boolean
  /** 侧栏跟随主题色（关 = 与内容区同底，融入式） */
  siderFollowPrimary: boolean
}

export const DEFAULT_APPEARANCE: AppearanceState = {
  themeMode: 'light',
  primaryColor: '#1677ff',
  radius: 6,
  compact: false,
  siderFollowPrimary: false,
}

const LS_KEY = 'dsh-relay.appearance'

/** 主色 → 同色相浅色变体（亮色模式侧栏跟随态）：保留色相，饱和 45%、明度 93% */
function primaryToLightTint(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex)
  if (!m) return '#f5f7fa'
  const [h, sat, lig] = hexToHsl(m[1], 0.45, 0.93)
  return hslToHex(h, sat, lig)
}

/** 主色 → 同色相深色变体（暗黑模式侧栏底色）：饱和 50%、明度 13% */
function primaryToDeepTint(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex)
  if (!m) return '#001529'
  const [h, sat, lig] = hexToHsl(m[1], 0.5, 0.13)
  return hslToHex(h, sat, lig)
}

function hexToHsl(hex6: string, sat: number, lig: number): [number, number, number] {
  const n = parseInt(hex6, 16)
  const r = ((n >> 16) & 0xff) / 255
  const g = ((n >> 8) & 0xff) / 255
  const b = (n & 0xff) / 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const d = max - min
  if (d === 0) return [0, 0, lig]
  let h: number
  if (max === r) h = 60 * (((g - b) / d) % 6)
  else if (max === g) h = 60 * ((b - r) / d + 2)
  else h = 60 * ((r - g) / d + 4)
  if (h < 0) h += 360
  return [h, sat, lig]
}

function hslToHex(h: number, sat: number, lig: number): string {
  const c = (1 - Math.abs(2 * lig - 1)) * sat
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1))
  const m = lig - c / 2
  const [rr, gg, bb] =
    h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x]
  const to255 = (v: number) => Math.round((v + m) * 255)
  return `#${((to255(rr) << 16) | (to255(gg) << 8) | to255(bb)).toString(16).padStart(6, '0')}`
}

function load(): AppearanceState {
  try {
    const raw = localStorage.getItem(LS_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      return {
        themeMode: parsed.themeMode ?? 'light',
        // hex 守卫：localStorage 损坏时不让非法色注入 CSS 变量与 AntDV token
        primaryColor: /^#[0-9a-f]{6}$/i.test(parsed.primaryColor ?? '')
          ? parsed.primaryColor
          : '#1677ff',
        radius: RADIUS_OPTIONS.includes(parsed.radius) ? parsed.radius : 6,
        compact: parsed.compact === true,
        siderFollowPrimary: parsed.siderFollowPrimary === true,
      }
    }
  } catch {
    /* 损坏则回默认 */
  }
  return { ...DEFAULT_APPEARANCE }
}

export const useAppStore = defineStore('app', {
  state: () => ({ ...load(), systemDark: false }),
  getters: {
    /** 实际生效的暗黑态（auto 跟随系统） */
    isDark(state): boolean {
      if (state.themeMode === 'auto') return state.systemDark
      return state.themeMode === 'dark'
    },
  },
  actions: {
    persist() {
      localStorage.setItem(
        LS_KEY,
        JSON.stringify({
          themeMode: this.themeMode,
          primaryColor: this.primaryColor,
          radius: this.radius,
          compact: this.compact,
          siderFollowPrimary: this.siderFollowPrimary,
        }),
      )
    },
    setThemeMode(mode: ThemeMode) {
      this.themeMode = mode
      this.persist()
      this.applyGlobalTheme()
    },
    setPrimaryColor(color: string) {
      this.primaryColor = color
      this.persist()
      this.applyGlobalTheme()
    },
    setRadius(radius: number) {
      this.radius = radius
      this.persist()
      this.applyGlobalTheme()
    },
    setCompact(compact: boolean) {
      this.compact = compact
      this.persist()
    },
    setSiderFollowPrimary(v: boolean) {
      this.siderFollowPrimary = v
      this.persist()
      this.applyGlobalTheme()
    },
    /** 外观全部恢复默认 */
    resetAppearance() {
      this.themeMode = DEFAULT_APPEARANCE.themeMode
      this.primaryColor = DEFAULT_APPEARANCE.primaryColor
      this.radius = DEFAULT_APPEARANCE.radius
      this.compact = DEFAULT_APPEARANCE.compact
      this.siderFollowPrimary = DEFAULT_APPEARANCE.siderFollowPrimary
      this.persist()
      this.applyGlobalTheme()
    },
    /** 同步 html.dark 类与 CSS 变量（AntDV 算法在 App.vue 响应式计算） */
    applyGlobalTheme() {
      const dark = this.isDark
      document.documentElement.classList.toggle('dark', dark)
      document.documentElement.style.setProperty('--app-primary', this.primaryColor)
      document.documentElement.style.setProperty('--app-primary-hover', this.hoverOf(this.primaryColor))
      document.documentElement.style.setProperty('--app-radius', `${this.radius}px`)
      const siderBg = this.siderFollowPrimary
        ? dark
          ? primaryToDeepTint(this.primaryColor)
          : primaryToLightTint(this.primaryColor)
        : dark
          ? '#0a0c10'
          : '#f5f7fa'
      document.documentElement.style.setProperty('--app-sider-bg', siderBg)
      document.documentElement.style.setProperty('--app-sider-fg', dark ? '#ffffff' : '#262626')
    },
    /** 主色 hover 近似色（亮度 +8%），供 CSS 变量使用 */
    hoverOf(hex: string): string {
      if (!/^#[0-9a-f]{6}$/i.test(hex)) return hex
      const n = parseInt(hex.slice(1), 16)
      const clamp = (v: number) => Math.min(255, Math.round(v))
      const r = clamp(((n >> 16) & 0xff) * 1.08 + 10)
      const g = clamp(((n >> 8) & 0xff) * 1.08 + 10)
      const b = clamp((n & 0xff) * 1.08 + 10)
      return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, '0')}`
    },
    /** 监听系统主题变化（auto 模式实时跟随） */
    watchSystem() {
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
        this.systemDark = e.matches
        if (this.themeMode === 'auto') this.applyGlobalTheme()
      })
    },
    init() {
      this.systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches
      this.applyGlobalTheme()
      this.watchSystem()
    },
  },
})
