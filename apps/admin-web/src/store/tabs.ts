import { defineStore } from 'pinia'

/** 多页签（tabs-view）条目 */
export interface TabItem {
  /** 路由全路径，作为页签唯一标识 */
  path: string
  title: string
  /** 首页固定页签不可关闭 */
  closable: boolean
  /** 用户固定的页签：不显示关闭钮，关闭其他/全部时保留 */
  pinned?: boolean
}

const STORAGE_KEY = 'dsh-relay.tabs'

function load(): TabItem[] {
  try {
    const raw = sessionStorage.getItem(STORAGE_KEY)
    const parsed = raw ? JSON.parse(raw) : []
    // sessionStorage 被污染成非数组时降级为空，防 tabs.some 抛错白屏
    return Array.isArray(parsed) ? (parsed as TabItem[]) : []
  } catch {
    return []
  }
}

export const useTabsStore = defineStore('tabs', {
  state: () => ({
    tabs: load(),
    /** 首页路径（固定页签） */
    homePath: '/dashboard',
    /** 当前激活页路径（由布局组件同步维护） */
    currentPath: '',
  }),
  actions: {
    persist() {
      sessionStorage.setItem(STORAGE_KEY, JSON.stringify(this.tabs))
    },
    setHome(path: string, title: string) {
      this.homePath = path
      for (const t of this.tabs) {
        if (t.closable === false && t.path !== path) t.closable = true
      }
      const existing = this.tabs.find((t) => t.path === path)
      if (existing) {
        existing.closable = false
        existing.title = title
      } else {
        this.tabs.unshift({ path, title, closable: false })
      }
      this.persist()
    },
    /** 记录访问页；已存在则同步最新标题 */
    addTab(path: string, title: string) {
      if (!path || path === '/login') return
      const existing = this.tabs.find((t) => t.path === path)
      if (existing) {
        if (existing.title !== title) {
          existing.title = title
          this.persist()
        }
        return
      }
      this.tabs.push({ path, title, closable: path !== this.homePath })
      this.persist()
    },
    setCurrent(path: string) {
      this.currentPath = path
    },
    /** 关闭页签；若关闭的是当前页，返回应跳转的相邻页路径 */
    removeTab(path: string): string | null {
      const idx = this.tabs.findIndex((t) => t.path === path)
      if (idx < 0) return null
      if (!this.tabs[idx].closable) return null
      const wasCurrent = path === this.currentPath
      this.tabs.splice(idx, 1)
      this.persist()
      if (!wasCurrent) return null
      const next = this.tabs[Math.min(idx, this.tabs.length - 1)]
      return next?.path ?? this.homePath
    },
    /** 固定/取消固定（首页恒固定，不可操作） */
    togglePin(path: string) {
      const tab = this.tabs.find((t) => t.path === path)
      if (!tab || !tab.closable) return
      tab.pinned = !tab.pinned
      this.persist()
    },
    /** 关闭其他可关页签（保留固定页与指定页） */
    clearOthers(keep: string) {
      this.tabs = this.tabs.filter((t) => !t.closable || t.pinned || t.path === keep)
      this.persist()
    },
    /** 关闭 anchor 左/右侧可关页签；当前页被关时返回 anchor 路径 */
    closeSide(anchor: string, side: 'left' | 'right'): string | null {
      const idx = this.tabs.findIndex((t) => t.path === anchor)
      if (idx < 0) return null
      const range = side === 'left' ? [0, idx] : [idx + 1, this.tabs.length]
      const doomed = new Set(
        this.tabs
          .slice(range[0], range[1])
          .filter((t) => t.closable && !t.pinned)
          .map((t) => t.path),
      )
      if (!doomed.size) return null
      const currentClosed = doomed.has(this.currentPath)
      this.tabs = this.tabs.filter((t) => !doomed.has(t.path))
      this.persist()
      return currentClosed ? anchor : null
    },
    /** 关闭全部可关页签（固定页保留），返回应跳转的路径 */
    clearAll(): string {
      this.tabs = this.tabs.filter((t) => !t.closable || t.pinned)
      this.persist()
      return this.tabs.find((t) => t.path === this.currentPath)?.path ?? this.homePath
    },
    reset() {
      this.tabs = []
      this.currentPath = ''
      sessionStorage.removeItem(STORAGE_KEY)
    },
  },
})
