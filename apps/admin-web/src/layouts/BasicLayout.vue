<template>
  <a-layout style="min-height: 100vh">
    <a-layout-sider v-model:collapsed="collapsed" collapsible>
      <div class="logo">dsh-relay</div>
      <a-menu
        v-model:selectedKeys="selectedKeys"
        theme="dark"
        mode="inline"
        @click="onMenuClick"
      >
        <a-menu-item v-for="item in menus" :key="item.path">
          <component :is="item.icon" />
          <span>{{ item.title }}</span>
        </a-menu-item>
      </a-menu>
    </a-layout-sider>
    <a-layout>
      <a-layout-header class="header">
        <span class="title">DSH 中继管理台</span>
        <a-dropdown>
          <span class="user">
            <UserOutlined /> {{ auth.username || 'admin' }} <DownOutlined />
          </span>
          <template #overlay>
            <a-menu>
              <a-menu-item key="logout" @click="onLogout">
                <LogoutOutlined /> 退出登录
              </a-menu-item>
            </a-menu>
          </template>
        </a-dropdown>
      </a-layout-header>
      <a-layout-content class="content">
        <a-alert
          v-if="insecure"
          type="warning"
          show-icon
          banner
          message="当前经未加密 HTTP 传输，令牌与密码可被窃听。公网部署请置于 TLS 反代（Caddy/Nginx）之后。"
          style="margin-bottom: 16px"
        />
        <router-view />
      </a-layout-content>
      <a-layout-footer style="text-align: center; color: #999">
        dsh-relay-service · 协议 dsh-relay-v1
      </a-layout-footer>
    </a-layout>
  </a-layout>
</template>

<script setup lang="ts">
import { ref, watchEffect } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  DashboardOutlined,
  MobileOutlined,
  KeyOutlined,
  FileSearchOutlined,
  UserOutlined,
  DownOutlined,
  LogoutOutlined,
} from '@ant-design/icons-vue'
import { useAuthStore } from '@/store/auth'

const route = useRoute()
const router = useRouter()
const auth = useAuthStore()
const collapsed = ref(false)
const insecure = location.protocol !== 'https:'

const menus = [
  { path: '/dashboard', title: '概览', icon: DashboardOutlined },
  { path: '/devices', title: '设备管理', icon: MobileOutlined },
  { path: '/pairing', title: '配对码', icon: KeyOutlined },
  { path: '/audit', title: '审计日志', icon: FileSearchOutlined },
]

const selectedKeys = ref<string[]>([route.path])
watchEffect(() => {
  selectedKeys.value = [route.path]
})

function onMenuClick({ key }: { key: string | number }) {
  router.push(String(key))
}

async function onLogout() {
  await auth.logout()
  router.push('/login')
}
</script>

<style scoped>
.logo {
  height: 56px;
  margin: 12px;
  color: #fff;
  font-size: 18px;
  font-weight: 700;
  line-height: 56px;
  text-align: center;
  letter-spacing: 1px;
}
.header {
  background: #fff;
  padding: 0 24px;
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.title {
  font-size: 16px;
  font-weight: 600;
}
.user {
  cursor: pointer;
}
.content {
  margin: 20px;
  padding: 20px;
  background: #f5f6f8;
  border-radius: 8px;
  min-height: 360px;
}
</style>
