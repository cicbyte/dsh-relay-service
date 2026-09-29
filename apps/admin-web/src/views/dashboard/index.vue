<template>
  <div>
    <a-row :gutter="16" style="margin-bottom: 16px">
      <a-col v-for="s in stats" :key="s.title" :xs="12" :sm="8" :md="6" :lg="4">
        <a-card>
          <a-statistic :title="s.title" :value="s.value" :value-style="{ fontSize: '26px' }" />
        </a-card>
      </a-col>
    </a-row>

    <a-card title="运行信息" :loading="loading">
      <a-descriptions :column="2" bordered size="small">
        <a-descriptions-item label="鉴权模式">{{ overview?.authMode ?? '-' }}</a-descriptions-item>
        <a-descriptions-item label="服务版本">{{ health?.version ?? '-' }}</a-descriptions-item>
        <a-descriptions-item label="WS 监听">{{ overview?.listenWs ?? '-' }}</a-descriptions-item>
        <a-descriptions-item label="管理面监听">{{ overview?.listenAdmin ?? '-' }}</a-descriptions-item>
        <a-descriptions-item label="数据库">{{ health?.db ?? '-' }}</a-descriptions-item>
        <a-descriptions-item label="在线 Host / Client">
          {{ overview?.ws.hosts ?? 0 }} / {{ overview?.ws.clients ?? 0 }}
        </a-descriptions-item>
      </a-descriptions>
      <a-button style="margin-top: 16px" @click="load">刷新</a-button>
    </a-card>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import type { Overview, HealthDetail } from '@/api/status'
import { getOverview, getHealth } from '@/api/status'

const overview = ref<Overview>()
const health = ref<HealthDetail>()
const loading = ref(false)

const stats = computed(() => [
  { title: '设备总数', value: overview.value?.devicesTotal ?? 0 },
  { title: '在线设备', value: overview.value?.devicesOnline ?? 0 },
  { title: '在线房间', value: overview.value?.ws.rooms ?? 0 },
  { title: '吊销设备', value: overview.value?.devicesRevoked ?? 0 },
  { title: '活跃配对码', value: overview.value?.pairingActive ?? 0 },
  { title: '数据库', value: health.value?.db ?? '-' },
])

async function load() {
  loading.value = true
  try {
    const [o, h] = await Promise.all([getOverview(), getHealth()])
    overview.value = o
    health.value = h
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>
