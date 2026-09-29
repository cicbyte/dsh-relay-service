<script setup lang="ts">
import { computed, h, ref, watch, type Component } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  DashboardOutlined, SettingOutlined, LogoutOutlined, HomeOutlined,
  DownOutlined, SkinOutlined, MenuOutlined,
  MobileOutlined, KeyOutlined, FileSearchOutlined, ApiOutlined,
} from '@ant-design/icons-vue'
import { SunIcon, MoonIcon } from '@/components/ThemeIcons'
import { useAuthStore } from '@/store/auth'
import { useTabsStore } from '@/store/tabs'
import { useAppStore } from '@/store/app'
import TabsView from './TabsView.vue'
import SettingsDrawer from './SettingsDrawer.vue'
import type { ItemType } from 'ant-design-vue/es/menu'

/**
 * 主布局（对齐 byte-admin）：
 * - 侧边栏：一级目录可展开/收起二级菜单，支持整体折叠（顶栏触发器）
 * - 顶栏：折叠触发器 + 面包屑 + 暗黑快捷切换/设置入口/用户下拉
 * - 多页签条（TabsView）+ 内容区
 */
const ICONS: Record<string, Component> = {
  DashboardOutlined, SettingOutlined, MobileOutlined, KeyOutlined,
  FileSearchOutlined, ApiOutlined,
}

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const tabsStore = useTabsStore()
const appStore = useAppStore()

// ---------- 菜单（静态两级） ----------
interface AntMenuItem {
  key: string
  label: string
  icon?: unknown
  children?: AntMenuItem[]
}

const MENU: AntMenuItem[] = [
  {
    key: '/dashboard',
    label: '概览',
    icon: () => h(ICONS.DashboardOutlined),
  },
  {
    key: '/channel',
    label: '通道管理',
    icon: () => h(ICONS.ApiOutlined),
    children: [
      { key: '/devices', label: '设备管理', icon: () => h(ICONS.MobileOutlined) },
      { key: '/pairing', label: '配对码', icon: () => h(ICONS.KeyOutlined) },
    ],
  },
  {
    key: '/system',
    label: '系统管理',
    icon: () => h(ICONS.SettingOutlined),
    children: [
      { key: '/audit', label: '审计日志', icon: () => h(ICONS.FileSearchOutlined) },
    ],
  },
]

const menuItems = computed<ItemType[]>(() => MENU as unknown as ItemType[])

// ---------- 侧边栏 ----------
const collapsed = ref(false)
const selectedKeys = computed(() => [route.path])
const openKeys = ref<string[]>([])

/** 刷新/跳转后展开当前路由所属目录 */
watch(
  [() => route.path, menuItems],
  ([path]) => {
    const tops = MENU
    for (const top of tops) {
      if (top.children?.some((c) => c.key === path)) {
        if (!openKeys.value.includes(top.key)) openKeys.value.push(top.key)
      }
    }
  },
  { immediate: true },
)

/** 菜单重建纪元：items 更新或折叠态翻转时强制重建 a-menu——
 *  规避 antd inline 菜单在折叠过渡中注入 items 卡空白的竞态 */
const menuEpoch = ref(0)
watch([menuItems, collapsed], () => {
  menuEpoch.value++
})

function onMenuClick({ key }: { key: string | number }) {
  const k = String(key)
  // 只有带子菜单的目录不导航（交给 a-menu 自身展开/收起）；一级叶子（概览）正常跳转
  const dir = MENU.find((top) => top.key === k)
  if (dir?.children?.length) return
  if (k !== route.path) router.push(k)
}

// ---------- 面包屑（由菜单树推导，含分组名） ----------
const breadcrumbs = computed(() => {
  const title = route.meta?.title as string | undefined
  if (!title) return []
  const parent = route.meta?.parent as string | undefined
  return parent ? [{ title: parent }, { title }] : [{ title }]
})

// ---------- 外观 ----------
const settingsOpen = ref(false)

function toggleDark() {
  appStore.setThemeMode(appStore.isDark ? 'light' : 'dark')
}

// ---------- 用户菜单 ----------
async function onLogout() {
  await auth.logout()
  tabsStore.reset()
  router.push('/login')
}

const displayName = computed(() => auth.username || 'admin')
const firstChar = computed(() => displayName.value.slice(0, 1).toUpperCase())
</script>

<template>
  <a-layout class="app-layout">
    <a-layout-sider
      v-model:collapsed="collapsed"
      collapsible
      :trigger="null"
      :width="220"
      :collapsed-width="64"
      class="app-sider"
      :style="{ background: 'var(--app-sider-bg)' }"
      breakpoint="lg"
    >
      <div class="logo">
        <div class="logo-mark">D</div>
        <span v-if="!collapsed" class="logo-text">dsh-relay</span>
      </div>
      <a-menu
        :key="menuEpoch"
        mode="inline"
        :theme="appStore.isDark ? 'dark' : 'light'"
        :items="menuItems"
        v-model:selected-keys="selectedKeys"
        v-model:open-keys="openKeys"
        @click="onMenuClick"
      />
    </a-layout-sider>

    <a-layout class="app-body">
      <a-layout-header class="app-header">
        <div class="header-left">
          <span class="collapse-trigger" @click="collapsed = !collapsed">
            <MenuOutlined />
          </span>
          <a-breadcrumb>
            <a-breadcrumb-item>
              <RouterLink to="/dashboard"><HomeOutlined /> 首页</RouterLink>
            </a-breadcrumb-item>
            <a-breadcrumb-item v-for="b in breadcrumbs" :key="b.title">
              {{ b.title }}
            </a-breadcrumb-item>
          </a-breadcrumb>
        </div>
        <div class="header-right">
          <span
            class="header-icon-btn"
            :title="appStore.isDark ? '切换亮色' : '切换暗色'"
            @click="toggleDark"
          >
            <SunIcon v-if="!appStore.isDark" />
            <MoonIcon v-else />
          </span>
          <span class="header-icon-btn" title="外观设置" @click="settingsOpen = true">
            <SkinOutlined />
          </span>
          <a-dropdown :trigger="['click']">
            <div class="user-chip">
              <a-avatar :size="28" class="user-avatar">{{ firstChar }}</a-avatar>
              <span class="user-name">{{ displayName }}</span>
              <DownOutlined class="user-arrow" />
            </div>
            <template #overlay>
              <a-menu>
                <a-menu-item key="logout" @click="onLogout">
                  <LogoutOutlined /> 退出登录
                </a-menu-item>
              </a-menu>
            </template>
          </a-dropdown>
        </div>
      </a-layout-header>

      <TabsView />

      <a-layout-content class="app-content">
        <router-view />
      </a-layout-content>

      <a-layout-footer class="app-footer">
        dsh-relay-service · 协议 dsh-relay-v1
      </a-layout-footer>
    </a-layout>
  </a-layout>

  <SettingsDrawer v-model:open="settingsOpen" />
</template>

<style scoped>
.app-layout {
  height: 100%;
}

.app-sider {
  border-right: 1px solid var(--app-border);
}

.app-sider :deep(.ant-layout-sider-children) {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* 菜单区滚动条隐藏：折叠动画期间瞬态溢出不产生滚动条 */
.app-sider :deep(.ant-menu) {
  background: transparent !important;
  flex: 1;
  overflow-y: auto;
  overflow-x: clip;
  scrollbar-width: none;
  -ms-overflow-style: none;
}

.app-sider :deep(.ant-menu::-webkit-scrollbar) {
  display: none;
}

.app-sider :deep(.ant-menu .ant-menu-sub) {
  overflow: hidden;
  background: transparent !important;
}

.logo {
  height: 56px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--app-sider-fg);
  overflow: hidden;
  flex-shrink: 0;
}

.logo-mark {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  background: linear-gradient(135deg, var(--app-primary), var(--app-primary-hover));
  color: #fff;
  font-weight: 700;
  font-size: 18px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.logo-text {
  font-size: 16px;
  font-weight: 600;
  white-space: nowrap;
}

.app-body {
  min-width: 0;
  background: var(--app-bg);
}

.app-header {
  height: 56px;
  line-height: 56px;
  background: var(--app-header-bg);
  padding: 0 16px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--app-border);
}

.header-left {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 4px;
}

.header-icon-btn {
  font-size: 15px;
  color: var(--app-text-secondary);
  cursor: pointer;
  padding: 6px;
  border-radius: var(--app-radius, 6px);
  display: flex;
  align-items: center;
}

.header-icon-btn:hover {
  background: var(--app-hover-bg);
  color: var(--app-primary);
}

.collapse-trigger {
  font-size: 16px;
  color: var(--app-text-secondary);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
}

.collapse-trigger:hover {
  background: var(--app-hover-bg);
  color: var(--app-primary);
}

.user-chip {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  padding: 4px 10px;
  border-radius: 6px;
}

.user-chip:hover {
  background: var(--app-hover-bg);
}

.user-avatar {
  background: var(--app-primary);
  font-weight: 600;
}

.user-name {
  font-size: 14px;
  color: var(--app-text);
}

.user-arrow {
  font-size: 10px;
  color: var(--app-text-muted);
}

.app-content {
  margin: 12px 16px 0;
  padding: 16px;
  background: var(--app-card-bg);
  border-radius: calc(var(--app-radius, 6px) + 2px);
  overflow: auto;
  flex: 1;
  min-height: 0;
}

.app-footer {
  text-align: center;
  color: var(--app-text-muted);
  font-size: 12px;
  padding: 10px 16px;
  background: transparent;
}
</style>
