<template>
  <div>
    <ProTable
      ref="tableRef"
      table-key="relay-devices"
      row-key="id"
      :columns="columns"
      :fetch="fetchDevices"
    >
      <template #toolBar>
        <a-button type="primary" @click="goPairing">
          <KeyOutlined /> 生成配对码
        </a-button>
      </template>
      <template #bodyCell="{ column, record }">
        <template v-if="column.key === 'status'">
          <a-tag v-if="record.revoked" color="red">已吊销</a-tag>
          <a-tag v-else-if="record.online" color="green">在线</a-tag>
          <a-tag v-else>离线</a-tag>
        </template>
        <template v-else-if="column.key === 'role'">
          <a-tag :color="record.role === 'host' ? 'blue' : 'purple'">{{ record.role }}</a-tag>
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
            <a-popconfirm title="确认删除该设备？" ok-type="danger" @confirm="onDelete(record)">
              <a-button size="small" danger>删除</a-button>
            </a-popconfirm>
          </a-space>
        </template>
      </template>
    </ProTable>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { message, Modal } from 'ant-design-vue'
import { KeyOutlined } from '@ant-design/icons-vue'
import ProTable, { type ProColumn } from '@/components/ProTable/index.vue'
import type { DeviceView } from '@/api/devices'
import { listDevices, revokeDevice, rotateDevice, deleteDevice } from '@/api/devices'

const router = useRouter()
const tableRef = ref<{ reload: () => void; refresh: () => void } | null>(null)

const columns: ProColumn[] = [
  { title: '设备', dataIndex: 'name', key: 'name', width: 160, ellipsis: true },
  { title: 'ID', dataIndex: 'id', key: 'id', width: 190 },
  { title: '角色', dataIndex: 'role', key: 'role', width: 90 },
  { title: '房间', dataIndex: 'room', key: 'room', width: 110 },
  { title: '状态', key: 'status', width: 100, hideable: false },
  { title: '创建时间', dataIndex: 'createdAt', key: 'createdAt', width: 170 },
  { title: '最近活跃', dataIndex: 'lastSeenAt', key: 'lastSeenAt', width: 170 },
  { title: '操作', key: 'action', width: 250, hideable: false },
]

/** 数据量小：全量拉取 + 前端分页（ProTable 分页托管） */
async function fetchDevices({ pageNum, pageSize }: { pageNum: number; pageSize: number }) {
  const all = await listDevices()
  return {
    list: all.slice((pageNum - 1) * pageSize, pageNum * pageSize),
    total: all.length,
  }
}

function goPairing() {
  router.push('/pairing')
}

function onRevoke(record: DeviceView) {
  Modal.confirm({
    title: '吊销设备',
    content: `吊销 ${record.name || record.id}？该设备将立即被踢下线，令牌作废。`,
    okType: 'danger',
    async onOk() {
      await revokeDevice(record.id)
      message.success('已吊销')
      tableRef.value?.refresh()
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
      tableRef.value?.refresh()
    },
  })
}

async function onDelete(record: DeviceView) {
  await deleteDevice(record.id)
  message.success('已删除')
  tableRef.value?.refresh()
}
</script>
