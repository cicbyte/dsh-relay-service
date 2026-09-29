<script setup lang="ts">
// 应用根组件：ConfigProvider 提供组件文案与主题（亮/暗算法 + 主色 token）
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { theme as antdTheme } from 'ant-design-vue'
import zhCN from 'ant-design-vue/es/locale/zh_CN'
import { useAppStore } from '@/store/app'
import { useAuthStore } from '@/store/auth'
import { useTabsStore } from '@/store/tabs'
import router from '@/router'
import { REFRESH_KEY, TOKEN_KEY } from '@/utils/request'

const app = useAppStore()
onMounted(() => app.init())

const themeConfig = computed(() => {
  const base = app.isDark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm
  return {
    // 算法数组：紧凑叠加在明暗算法之上
    algorithm: app.compact ? [base, antdTheme.compactAlgorithm] : base,
    token: { colorPrimary: app.primaryColor, borderRadius: app.radius },
    locale: zhCN,
  }
})

// 多标签同步：其它标签登出（清 token）后本标签被动登出
function onStorage(e: StorageEvent) {
  const tokenGone =
    e.key === null || e.key === TOKEN_KEY || e.key === REFRESH_KEY
      ? !localStorage.getItem(TOKEN_KEY)
      : false
  if (tokenGone) {
    const auth = useAuthStore()
    if (auth.loggedIn) {
      auth.logout()
      useTabsStore().reset()
      router.push('/login')
    }
  }
}
onMounted(() => window.addEventListener('storage', onStorage))
onBeforeUnmount(() => window.removeEventListener('storage', onStorage))
</script>

<template>
  <a-config-provider :theme="themeConfig">
    <router-view />
  </a-config-provider>
</template>

<style>
:root {
  --app-primary: #1677ff;
  --app-primary-hover: #4096ff;
  /* 布局语义色（亮色默认） */
  --app-bg: #f5f7fa;
  --app-header-bg: #ffffff;
  --app-card-bg: #ffffff;
  --app-text: #262626;
  --app-text-secondary: #595959;
  --app-text-muted: #8c8c8c;
  --app-border: #f0f0f0;
  --app-hover-bg: #f5f5f5;
  --app-elevated-bg: #ffffff;
  --app-radius: 6px;
  --app-sider-bg: #001529;
  --app-sider-fg: #ffffff;
}

html.dark {
  --app-bg: #0a0c10;
  --app-header-bg: #141414;
  --app-card-bg: #141414;
  --app-text: #e5e6eb;
  --app-text-secondary: #a9aeb8;
  --app-text-muted: #6b7785;
  --app-border: #262626;
  --app-hover-bg: #1f1f1f;
  --app-elevated-bg: #1f2329;
}

html,
body,
#app {
  height: 100%;
  margin: 0;
  padding: 0;
  /* 页面级滚动收进布局内容区，避免侧栏折叠过渡期 body 滚动条闪现 */
  overflow: hidden;
  background: var(--app-bg);
  color: var(--app-text);
}

/* 全站滚动条：6px 细条、默认透明（悬停显色） */
*::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}

*::-webkit-scrollbar-track {
  background: transparent;
}

*::-webkit-scrollbar-thumb {
  background: transparent;
  border-radius: 3px;
}

*:hover::-webkit-scrollbar-thumb {
  background: rgba(128, 128, 128, 0.35);
}

* {
  scrollbar-width: thin;
  scrollbar-color: transparent transparent;
}

*:hover {
  scrollbar-color: rgba(128, 128, 128, 0.45) transparent;
}

/* 自绘主题图标（太阳/月亮） */
.theme-icon {
  width: 1em;
  height: 1em;
  display: inline-block;
  vertical-align: -0.125em;
}
</style>
