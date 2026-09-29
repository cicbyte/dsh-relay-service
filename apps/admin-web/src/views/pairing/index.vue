<template>
  <a-card title="配对码">
    <a-alert
      type="info"
      show-icon
      message="一次性配对码 · 10 分钟内有效 · 单次使用 · 失败 5 次熔断"
      style="margin-bottom: 20px"
    />
    <a-form layout="inline" @finish="onIssue">
      <a-form-item label="角色">
        <a-select v-model:value="role" style="width: 180px">
          <a-select-option value="client">client（手机）</a-select-option>
          <a-select-option value="host">host（桌面桥）</a-select-option>
        </a-select>
      </a-form-item>
      <a-form-item label="设备名">
        <a-input v-model:value="name" placeholder="可选" style="width: 220px" />
      </a-form-item>
      <a-form-item>
        <a-button type="primary" html-type="submit" :loading="loading">生成配对码</a-button>
      </a-form-item>
    </a-form>

    <div v-if="lastCode" class="result">
      <a-typography-title :level="2" style="letter-spacing: 6px; margin: 24px 0 8px">
        {{ lastCode }}
      </a-typography-title>
      <a-typography-paragraph copyable :content="lastCode">
        有效期至 {{ fmtTime(expiresAt) }} · 角色 {{ role }}
      </a-typography-paragraph>
    </div>
  </a-card>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { message } from 'ant-design-vue'
import dayjs from 'dayjs'
import { issuePairing } from '@/api/pairing'

const role = ref<'client' | 'host'>('client')
const name = ref('')
const loading = ref(false)
const lastCode = ref('')
const expiresAt = ref(0)

function fmtTime(t: number) {
  return t ? dayjs(t * 1000).format('YYYY-MM-DD HH:mm:ss') : '-'
}

async function onIssue() {
  loading.value = true
  try {
    const out = await issuePairing(role.value, name.value.trim())
    lastCode.value = out.code
    expiresAt.value = out.expiresAt
    message.success('配对码已生成')
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.result {
  text-align: center;
  padding: 12px 0 24px;
}
</style>
