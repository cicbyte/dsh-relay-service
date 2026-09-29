<template>
  <div class="login-page">
    <a-card class="login-card" :bordered="false">
      <h1 class="brand">dsh-relay 管理台</h1>
      <p class="sub">DSH 中继服务 · 设备管理与审计</p>
      <a-form layout="vertical" :model="formState" @finish="onFinish">
        <a-form-item label="用户名" name="username">
          <a-input v-model:value="formState.username" size="large" placeholder="admin" autocomplete="username">
            <template #prefix><UserOutlined /></template>
          </a-input>
        </a-form-item>
        <a-form-item label="密码" name="password">
          <a-input-password
            v-model:value="formState.password"
            size="large"
            placeholder="密码"
            autocomplete="current-password"
            @pressEnter="onFinish"
          >
            <template #prefix><LockOutlined /></template>
          </a-input-password>
        </a-form-item>
        <a-button type="primary" size="large" block :loading="loading" html-type="submit">
          登 录
        </a-button>
      </a-form>
      <a-alert
        v-if="location.protocol !== 'https:'"
        type="warning"
        show-icon
        message="未加密 HTTP 连接"
        description="公网部署请置于 TLS 反代之后。"
        style="margin-top: 16px"
      />
    </a-card>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { message } from 'ant-design-vue'
import { UserOutlined, LockOutlined } from '@ant-design/icons-vue'
import { useAuthStore } from '@/store/auth'

const router = useRouter()
const route = useRoute()
const auth = useAuthStore()
const formState = reactive({ username: 'admin', password: '' })
const loading = ref(false)
const location = window.location

async function onFinish() {
  if (!formState.password) {
    message.warning('请输入密码')
    return
  }
  loading.value = true
  try {
    await auth.login(formState.username.trim(), formState.password)
    const redirect = String(route.query.redirect ?? '/dashboard')
    router.push(redirect)
  } catch (e) {
    // 错误提示已由 request 层弹出（登录接口 skipErrorToast，这里自定义）
    message.error(e instanceof Error ? e.message : '登录失败')
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.login-page {
  height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f0f2f5;
}
.login-card {
  width: 380px;
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.08);
}
.brand {
  margin: 8px 0 4px;
  font-size: 22px;
}
.sub {
  color: #888;
  margin-bottom: 24px;
}
</style>
