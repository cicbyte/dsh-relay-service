<template>
  <a-card title="审计日志">
    <template #extra>
      <a-space>
        <a-select v-model:value="limit" style="width: 120px" @change="load">
          <a-select-option :value="20">20 条</a-select-option>
          <a-select-option :value="50">50 条</a-select-option>
          <a-select-option :value="200">200 条</a-select-option>
        </a-select>
        <a-button @click="load">刷新</a-button>
      </a-space>
    </template>
    <a-table
      :data-source="rows"
      :columns="columns"
      row-key="id"
      :loading="loading"
      :pagination="{ pageSize: 20 }"
      size="middle"
    >
      <template #bodyCell="{ column, record }">
        <template v-if="column.key === 'ts'">
          <span class="muted">{{ fmtTime(record.ts) }}</span>
        </template>
        <template v-else-if="column.key === 'event'">
          <a-tag :color="eventColor(record.event)">{{ record.event }}</a-tag>
        </template>
        <template v-else-if="column.key === 'detail'">
          <span class="mono muted">{{ record.detail }}</span>
        </template>
      </template>
    </a-table>
  </a-card>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import dayjs from 'dayjs'
import type { AuditView } from '@/api/audit'
import { listAudit } from '@/api/audit'

const rows = ref<AuditView[]>([])
const loading = ref(false)
const limit = ref(50)

const columns = [
  { title: '时间', key: 'ts', width: 180 },
  { title: '事件', key: 'event', width: 150 },
  { title: 'IP', dataIndex: 'ip', key: 'ip', width: 140 },
  { title: '设备', dataIndex: 'device', key: 'device', width: 160 },
  { title: '详情', key: 'detail' },
]

function fmtTime(t: number) {
  return t ? dayjs(t * 1000).format('YYYY-MM-DD HH:mm:ss') : '-'
}

function eventColor(event: string) {
  if (event.startsWith('conn.')) return 'blue'
  if (event.startsWith('admin.')) return 'purple'
  return 'default'
}

async function load() {
  loading.value = true
  try {
    rows.value = await listAudit(limit.value)
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.muted {
  color: #888;
}
.mono {
  font-family: Consolas, monospace;
  font-size: 12px;
}
</style>
