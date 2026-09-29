<script setup lang="ts">
import { ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  CloseOutlined, DownOutlined, ReloadOutlined,
  ColumnWidthOutlined, CloseCircleOutlined,
  PushpinOutlined, PushpinFilled, LinkOutlined, VerticalRightOutlined, VerticalLeftOutlined,
} from '@ant-design/icons-vue'
import { message } from 'ant-design-vue'
import { useTabsStore, type TabItem } from '@/store/tabs'

/**
 * 多页签（对齐 byte-admin tabs-view）：
 * - 访问过的页面记录为页签（sessionStorage 持久化，刷新不丢）
 * - 首页固定不可关闭；支持关闭单页 / 关闭其他 / 关闭全部 / 刷新当前页
 */
const route = useRoute()
const router = useRouter()
const tabsStore = useTabsStore()

watch(
  () => route.path,
  (path) => {
    const title = (route.meta?.title as string) || ''
    tabsStore.setCurrent(path)
    if (title && path !== '/login') {
      tabsStore.addTab(path, title)
    }
  },
  { immediate: true },
)

function go(path: string) {
  if (path !== route.path) router.push(path)
}

function close(path: string) {
  const next = tabsStore.removeTab(path)
  if (next && next !== route.path) router.push(next)
}

function refresh() {
  router.replace({ path: '/redirect' + route.fullPath })
}

function clearOthers() {
  tabsStore.clearOthers(route.path)
}

// ---------- 右键菜单 ----------
const ctxTarget = ref<TabItem | null>(null)

function onContextMenu(tab: TabItem) {
  ctxTarget.value = tab
}

function ctxClose() {
  if (ctxTarget.value) close(ctxTarget.value.path)
}

function ctxTogglePin() {
  tabsStore.togglePin(ctxTarget.value?.path ?? '')
}

function ctxCloseSide(side: 'left' | 'right') {
  const anchor = ctxTarget.value?.path
  if (!anchor) return
  const next = tabsStore.closeSide(anchor, side)
  if (next && next !== route.path) router.push(next)
}

function ctxCopyLink() {
  const path = ctxTarget.value?.path
  if (!path) return
  navigator.clipboard
    .writeText(`${location.origin}${path}`)
    .then(() => message.success('链接已复制'))
    .catch(() => message.error('复制失败'))
}

function clearAll() {
  const home = tabsStore.clearAll()
  if (home && home !== route.path) router.push(home)
}
</script>

<template>
  <div class="tabs-view">
    <div class="tabs-scroll">
      <a-dropdown
        v-for="tab in tabsStore.tabs"
        :key="tab.path"
        :trigger="['contextmenu']"
      >
        <div
          class="tab-item"
          :class="{ active: tab.path === tabsStore.currentPath, pinned: tab.pinned }"
          @click="go(tab.path)"
          @contextmenu="onContextMenu(tab)"
        >
          <PushpinFilled v-if="tab.pinned" class="tab-pin" />
          <span class="tab-title" :title="tab.title">{{ tab.title }}</span>
          <CloseOutlined
            v-if="tab.closable && !tab.pinned"
            class="tab-close"
            @click.stop="close(tab.path)"
          />
        </div>
        <template #overlay>
          <a-menu>
            <a-menu-item key="refresh" :disabled="tab.path !== route.path" @click="refresh">
              <ReloadOutlined /> 刷新
            </a-menu-item>
            <a-menu-item
              v-if="tab.closable"
              key="pin"
              @click="onContextMenu(tab); ctxTogglePin()"
            >
              <PushpinOutlined /> {{ tab.pinned ? '取消固定' : '固定' }}
            </a-menu-item>
            <a-menu-divider />
            <a-menu-item key="close" :disabled="!tab.closable || tab.pinned" @click="onContextMenu(tab); ctxClose()">
              <CloseOutlined /> 关闭当前
            </a-menu-item>
            <a-menu-item key="left" @click="onContextMenu(tab); ctxCloseSide('left')">
              <VerticalRightOutlined /> 关闭左侧
            </a-menu-item>
            <a-menu-item key="right" @click="onContextMenu(tab); ctxCloseSide('right')">
              <VerticalLeftOutlined /> 关闭右侧
            </a-menu-item>
            <a-menu-item key="others" @click="onContextMenu(tab); tabsStore.clearOthers(tab.path)">
              <ColumnWidthOutlined /> 关闭其他
            </a-menu-item>
            <a-menu-item key="all" @click="clearAll">
              <CloseCircleOutlined /> 关闭全部
            </a-menu-item>
            <a-menu-divider />
            <a-menu-item key="copy" @click="onContextMenu(tab); ctxCopyLink()">
              <LinkOutlined /> 复制链接
            </a-menu-item>
          </a-menu>
        </template>
      </a-dropdown>
    </div>
    <a-dropdown>
      <div class="tabs-actions">
        <DownOutlined />
      </div>
      <template #overlay>
        <a-menu>
          <a-menu-item key="refresh" @click="refresh">
            <ReloadOutlined /> 刷新
          </a-menu-item>
          <a-menu-item key="others" @click="clearOthers">
            <ColumnWidthOutlined /> 关闭其他
          </a-menu-item>
          <a-menu-item key="all" @click="clearAll">
            <CloseCircleOutlined /> 关闭全部
          </a-menu-item>
        </a-menu>
      </template>
    </a-dropdown>
  </div>
</template>

<style scoped>
.tabs-view {
  display: flex;
  align-items: center;
  background: var(--app-header-bg);
  border-bottom: 1px solid var(--app-border);
  padding: 6px 8px 0 12px;
  gap: 4px;
}

.tabs-scroll {
  display: flex;
  align-items: center;
  gap: 6px;
  overflow-x: auto;
  flex: 1;
  min-width: 0;
}

.tabs-scroll::-webkit-scrollbar {
  height: 3px;
}

.tabs-scroll::-webkit-scrollbar-thumb {
  background: var(--app-border);
  border-radius: 2px;
}

.tab-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  font-size: 13px;
  color: var(--app-text-secondary);
  background: var(--app-hover-bg);
  border: 1px solid var(--app-border);
  border-radius: var(--app-radius, 6px) var(--app-radius, 6px) 0 0;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.2s;
  user-select: none;
}

.tab-item:hover {
  color: var(--app-primary);
}

.tab-item.active {
  color: var(--app-primary);
  background: var(--app-header-bg);
  border-color: var(--app-border);
  border-bottom-color: var(--app-header-bg);
  font-weight: 500;
}

.tab-item.active::before {
  content: '';
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--app-primary);
}

.tab-pin {
  font-size: 11px;
  color: var(--app-primary);
}

.tab-close {
  font-size: 10px;
  padding: 2px;
  border-radius: 50%;
  color: var(--app-text-muted);
}

.tab-close:hover {
  color: #fff;
  background: #bfbfbf;
}

.tab-item.active .tab-close:hover {
  background: var(--app-primary);
}

.tabs-actions {
  padding: 4px 8px;
  color: var(--app-text-secondary);
  cursor: pointer;
  border-radius: 4px;
  margin-bottom: 4px;
}

.tabs-actions:hover {
  color: var(--app-primary);
  background: var(--app-hover-bg);
}
</style>
