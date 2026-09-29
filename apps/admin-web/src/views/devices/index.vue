<template>
  <a-card title="设备管理">
    <template #extra>
      <a-button @click="load">刷新</a-button>
    </template>
    <a-table
      :data-source="devices"
      :columns="columns"
      row-key="id"
      :loading="loading"
      :pagination="{ pageSize: 10 }"
      size="middle"
    >
      <template #bodyCell="{ column, record }">
        <template v-if="column.key === 'status'">
          <a-tag v-if="record.revoked" color="red">已吊销</a-tag>
          <a-tag v-else-if="record.online" color="green">在线</a-tag>
          <a-tag v-else>离线</a-tag>
        </template>
        <template v-else-if="column.key === 'time'">
          <span class="muted">{{ fmtTime(record.lastSeenAt) }}</span>
        </template>
        <template v-else-if="column.key === 'action'">
          <a-space>
            <a-button
              v-if="!record.revoked"
              size="small"
              danger
              @click="onRevoke(record)"
            >
              吊销
            </a-button>
            <a-button size="small" @click="onRotate(record)">轮换令牌</a-button>
            <a-button size="small" danger @click="onDelete(record)">删除</a-button>
          </a-space>
        </template>
      </template>
    </a-table>
  </a-card>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { message, Modal } from 'ant-design-vue'
import dayjs from 'dayjs'
import type { DeviceView } from '@/api/devices'
import { listDevices, revokeDevice, rotateDevice, deleteDevice } from '@/api/devices'

const devices = ref<DeviceView[]>([])
const loading = ref(false)

const columns = [
  { title: '设备', dataIndex: 'name', key: 'name' },
  { title: 'ID', dataIndex: 'id', key: 'id' },
  { title: '角色', dataIndex: 'role', key: 'role', width: 80 },
  { title: '房间', dataIndex: 'room', key: 'room', width: 110 },
  { title: '状态', key: 'status', width: 100 },
  { title: '最近活跃', key: 'time', width: 170 },
  { title: '操作', key: 'action', width: 240 },
]

function fmtTime(t?: number) {
  return t ? dayjs(t * 1000).format('YYYY-MM-DD HH:mm:ss') : '-'
}

async function load() {
  loading.value = true
  try {
    devices.value = await listDevices()
  } finally {
    loading.value = false
  }
}

function onRevoke(record: DeviceView) {
  Modal.confirm({
    title: '吊销设备',
    content: `吊销 ${record.name || record.id}？该设备将立即被踢下线，令牌作废。`,
    okType: 'danger',
    async onOk() {
      await revokeDevice(record.id)
      message.success('已吊销')
      await load()
    },
  })
}

function onRotate(record: DeviceView) {
  Modal.confirm({
    title: '轮换令牌',
    content: `轮换 ${record.name || record.id} 的令牌？旧令牌立即失效。`,
    async onOk() {
      const out = await rotateDevice(record.id)
      Modal.info({
        title: '新令牌（只显示这一次，请立即保存）',
        content: out.token,
        width: 520,
      })
      await load()
    },
  })
}

function onDelete(record: DeviceView) {
  Modal.confirm({
    title: '删除设备',
    content: `删除 ${record.name || record.id}？操作不可恢复。`,
    okType: 'danger',
    async onOk() {
      await deleteDevice(record.id)
      message.success('已删除')
      await load()
    },
  })
}

onMounted(load)
</script>

<style scoped>
.muted {
  color: #888;
}
</style>
