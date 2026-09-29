<template>
  <ProTable
    ref="tableRef"
    table-key="relay-audit"
    row-key="id"
    :columns="columns"
    :fetch="fetchAudit"
    :page-size="20"
  >
    <template #toolBar>
      <a-space>
        <span class="muted">条数</span>
        <a-select v-model:value="limit" style="width: 120px" @change="reload">
          <a-select-option :value="20">20 条</a-select-option>
          <a-select-option :value="50">50 条</a-select-option>
          <a-select-option :value="200">200 条</a-select-option>
        </a-select>
      </a-space>
    </template>
    <template #bodyCell="{ column, record }">
      <template v-if="column.key === 'event'">
        <a-tag :color="eventColor(record.event)">{{ record.event }}</a-tag>
      </template>
      <template v-else-if="column.key === 'detail'">
        <span class="mono muted">{{ record.detail }}</span>
      </template>
    </template>
  </ProTable>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import ProTable, { type ProColumn } from '@/components/ProTable/index.vue'
import { listAudit } from '@/api/audit'

const limit = ref(50)
const tableRef = ref<{ reload: () => void; refresh: () => void } | null>(null)

const columns: ProColumn[] = [
  { title: '时间', dataIndex: 'ts', key: 'ts', width: 180 },
  { title: '事件', dataIndex: 'event', key: 'event', width: 150 },
  { title: 'IP', dataIndex: 'ip', key: 'ip', width: 140 },
  { title: '设备', dataIndex: 'device', key: 'device', width: 180 },
  { title: '详情', dataIndex: 'detail', key: 'detail', ellipsis: true },
]

function eventColor(event: string) {
  if (event.startsWith('conn.')) return 'blue'
  if (event.startsWith('admin.')) return 'purple'
  return 'default'
}

/** 时间倒序取前 N 条 + 前端分页 */
async function fetchAudit({ pageNum, pageSize }: { pageNum: number; pageSize: number }) {
  const all = await listAudit(limit.value)
  return {
    list: all.slice((pageNum - 1) * pageSize, pageNum * pageSize),
    total: all.length,
  }
}

function reload() {
  tableRef.value?.reload()
}
</script>

<style scoped>
.muted {
  color: var(--app-text-muted);
  font-size: 13px;
}
.mono {
  font-family: Consolas, monospace;
  font-size: 12px;
}
</style>
