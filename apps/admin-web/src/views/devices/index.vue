<template>
  <div class="env-page">
    <!-- 左：环境列表 -->
    <div class="env-list">
      <a-button type="primary" block @click="onCreateEnv">
        <PlusOutlined /> 新建环境
      </a-button>
      <div class="env-scroll">
        <div
          v-for="r in rooms"
          :key="r.room"
          class="env-item"
          :class="{ active: r.room === selectedRoom }"
          @click="selectedRoom = r.room"
        >
          <div class="env-name">
            <span class="dot" :class="{ on: r.hostOnline }" />
            {{ r.displayName }}
          </div>
          <div class="env-meta">
            <DesktopOutlined v-if="r.hostOnline" /> <DisconnectOutlined v-else />
            {{ r.clients.length }} 台设备 · {{ r.clientsOnline }} 在线
          </div>
        </div>
        <a-empty v-if="!rooms.length" description="暂无环境" :image-style="{ margin: '24px 0' }" />
      </div>
    </div>

    <!-- 右：环境详情 -->
    <div class="env-detail" v-if="current">
      <a-card class="head-card">
        <div class="head-row">
          <div>
            <h2 class="env-title">
              {{ current.displayName }}
              <a-button size="small" type="link" @click="onRename">
                <EditOutlined /> 重命名
              </a-button>
            </h2>
            <div class="env-sub">环境 ID {{ current.room }} · 创建于 {{ fmtDateTime(current.createdAt) }}</div>
          </div>
          <a-space>
            <a-button type="primary" @click="openPair('host')"><KeyOutlined /> Host 配对码</a-button>
            <a-button @click="openPair('client')"><MobileOutlined /> 手机配对码</a-button>
          </a-space>
        </div>
      </a-card>

      <a-card title="桌面桥（Host）" class="sec-card">
        <div v-if="current.host" class="dev-row">
          <div>
            <div class="dev-name">
              {{ current.host.name }}
              <a-tag v-if="current.host.revoked" color="red">已吊销</a-tag>
              <a-tag v-else-if="current.host.online" color="green">在线</a-tag>
              <a-tag v-else>离线</a-tag>
            </div>
            <div class="dev-sub">{{ current.host.id }} · 最近活跃 {{ fmtDateTime(current.host.lastSeenAt) }}</div>
          </div>
          <a-space>
            <a-button v-if="!current.host.revoked" size="small" danger @click="onRevoke(current.host)">吊销</a-button>
            <a-button size="small" @click="onRotate(current.host)">轮换令牌</a-button>
            <a-popconfirm title="确认删除该设备？" ok-type="danger" @confirm="onDelete(current.host!)">
              <a-button size="small" danger>删除</a-button>
            </a-popconfirm>
          </a-space>
        </div>
        <a-empty v-else description="尚未配对：点击右上「Host 配对码」扫码接入">
          <a-button type="primary" @click="openPair('host')">生成 Host 配对码</a-button>
        </a-empty>
      </a-card>

      <a-card title="手机 / 平板（Clients）" class="sec-card">
        <a-table
          :data-source="current.clients"
          :columns="clientColumns"
          row-key="id"
          size="middle"
          :pagination="false"
        >
          <template #bodyCell="{ column, record }">
            <template v-if="column.key === 'status'">
              <a-tag v-if="record.revoked" color="red">已吊销</a-tag>
              <a-tag v-else-if="record.online" color="green">在线</a-tag>
              <a-tag v-else>离线</a-tag>
            </template>
            <template v-else-if="column.key === 'lastSeenAt'">
              {{ fmtDateTime(record.lastSeenAt) }}
            </template>
            <template v-else-if="column.key === 'action'">
              <a-space>
                <a-button v-if="!record.revoked" size="small" danger @click="onRevoke(record)">吊销</a-button>
                <a-button size="small" @click="onRotate(record)">轮换令牌</a-button>
                <a-popconfirm title="确认删除该设备？" ok-type="danger" @confirm="onDelete(record)">
                  <a-button size="small" danger>删除</a-button>
                </a-popconfirm>
              </a-space>
            </template>
          </template>
        </a-table>
        <a-empty v-if="!current.clients.length" description="暂无客户端：生成手机配对码扫码接入" />
      </a-card>
    </div>
    <a-empty v-else class="env-empty" description="左侧选择或新建一个环境" />

    <!-- 出码 + 二维码 -->
    <a-modal v-model:open="pairOpen" :title="pairRole === 'host' ? 'Host 配对码' : '手机配对码'" :footer="null" width="560">
      <a-form layout="vertical" :model="pairForm" @finish="onIssue">
        <a-form-item label="中继地址（手机可达的 relay 地址）" name="addr">
          <a-input v-model:value="pairForm.addr" placeholder="1.2.3.4:8787" />
        </a-form-item>
        <a-form-item label="设备名" name="name">
          <a-input v-model:value="pairForm.name" :placeholder="pairRole === 'host' ? '如：家里电脑' : '如：小米14'" />
        </a-form-item>
        <a-button type="primary" html-type="submit" :loading="issuing">生成配对码</a-button>
      </a-form>
      <a-divider v-if="pairCode" />
      <template v-if="pairCode">
        <div class="pair-code">{{ pairCode }}</div>
        <div class="pair-tip">600 秒内有效 · 仅可使用一次 · 手机扫码或手动输入</div>
        <div class="qr-row">
          <img v-if="qrDataUrl" :src="qrDataUrl" alt="pairing qr" />
          <div class="qr-side">
            <div class="pair-payload">{{ pairPayload }}</div>
            <a-space>
              <a-button size="small" @click="copyText(pairPayload)">复制 payload</a-button>
              <a-button size="small" @click="copyText(pairCode)">复制配对码</a-button>
            </a-space>
          </div>
        </div>
      </template>
    </a-modal>

    <!-- 环境命名 -->
    <a-modal v-model:open="nameModal.open" :title="nameModal.title" @ok="onNameOk" width="420">
      <a-input
        v-model:value="nameModal.value"
        placeholder="环境名（如：家里、公司）——同一环境可接入一台桌面桥与多台手机"
        @press-enter="onNameOk"
      />
    </a-modal>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { message, Modal } from 'ant-design-vue'
import {
  DesktopOutlined, DisconnectOutlined, EditOutlined, KeyOutlined,
  MobileOutlined, PlusOutlined,
} from '@ant-design/icons-vue'
import QRCode from 'qrcode'
import { fmtDateTime } from '@/utils/format'
import type { RoomDevice, RoomView } from '@/api/rooms'
import { createRoom, listRooms, renameRoom } from '@/api/rooms'
import { issuePairing } from '@/api/pairing'
import { deleteDevice, revokeDevice, rotateDevice } from '@/api/devices'

const rooms = ref<RoomView[]>([])
const selectedRoom = ref('')

const current = computed(() => rooms.value.find((r) => r.room === selectedRoom.value) ?? null)

const clientColumns = [
  { title: '设备', dataIndex: 'name', key: 'name', ellipsis: true },
  { title: 'ID', dataIndex: 'id', key: 'id', width: 190 },
  { title: '状态', key: 'status', width: 100 },
  { title: '最近活跃', key: 'lastSeenAt', width: 170 },
  { title: '操作', key: 'action', width: 240 },
]

async function load() {
  rooms.value = await listRooms()
  if (!rooms.value.some((r) => r.room === selectedRoom.value)) {
    selectedRoom.value = rooms.value[0]?.room ?? ''
  }
}

onMounted(load)

function onCreateEnv() {
  nameModal.title = '新建环境'
  nameModal.value = ''
  nameModal.open = true
}

function onRename() {
  nameModal.title = '重命名环境'
  nameModal.value = current.value?.displayName ?? ''
  nameModal.open = true
}

async function onNameOk() {
  const name = nameModal.value.trim()
  if (!name) return
  if (nameModal.title === '新建环境') {
    const room = await createRoom(name)
    await load()
    selectedRoom.value = room.room
    message.success('环境已创建')
  } else if (current.value) {
    await renameRoom(current.value.room, name)
    await load()
    message.success('已重命名')
  }
  nameModal.open = false
}

// ---- 出码 + QR ----
const pairOpen = ref(false)
const pairRole = ref<'host' | 'client'>('client')
const pairForm = reactive({ addr: `${window.location.hostname}:8787`, name: '' })
const pairCode = ref('')
const qrDataUrl = ref('')

const nameModal = reactive({ open: false, title: '新建环境', value: '' })

const pairPayload = computed(() => {
  const proto = pairForm.addr.startsWith('wss') || window.location.protocol === 'https:' ? 'dshrelays' : 'dshrelay'
  const addr = pairForm.addr.replace(/^wss?:\/\//, '')
  return `${proto}://${addr}/?pair=${pairCode.value}&room=${selectedRoom.value}&name=${encodeURIComponent(current.value?.displayName ?? '')}`
})

// 地址改了重生成二维码
watch(
  () => pairForm.addr,
  async () => {
    if (pairCode.value) {
      qrDataUrl.value = await QRCode.toDataURL(pairPayload.value, { width: 220, margin: 1 })
    }
  },
)

function openPair(role: 'host' | 'client') {
  pairRole.value = role
  pairForm.name = ''
  pairCode.value = ''
  qrDataUrl.value = ''
  pairOpen.value = true
}

const issuing = ref(false)
async function onIssue() {
  if (!current.value) return
  issuing.value = true
  try {
    const out = await issuePairing(pairRole.value, pairForm.name.trim(), current.value.room)
    pairCode.value = out.code
    qrDataUrl.value = await QRCode.toDataURL(pairPayload.value, { width: 220, margin: 1 })
  } finally {
    issuing.value = false
  }
}

async function copyText(text: string) {
  await navigator.clipboard.writeText(text)
  message.success('已复制')
}

// ---- 设备操作 ----
function onRevoke(record: RoomDevice) {
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

function onRotate(record: RoomDevice) {
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

async function onDelete(record: RoomDevice) {
  await deleteDevice(record.id)
  message.success('已删除')
  await load()
}
</script>

<style scoped>
.env-page {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}
.env-list {
  width: 264px;
  flex: none;
}
.env-scroll {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.env-item {
  padding: 10px 12px;
  border: 1px solid var(--app-border);
  border-radius: var(--app-radius);
  cursor: pointer;
  background: var(--app-bg-elevated);
}
.env-item:hover {
  border-color: var(--app-primary);
}
.env-item.active {
  border-color: var(--app-primary);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--app-primary) 18%, transparent);
}
.env-name {
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}
.env-name .dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--app-text-tertiary);
}
.env-name .dot.on {
  background: #52c41a;
}
.env-meta {
  margin-top: 4px;
  font-size: 12px;
  color: var(--app-text-secondary);
}
.env-detail {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.env-empty {
  flex: 1;
  padding-top: 80px;
}
.head-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
}
.env-title {
  margin: 0;
  font-size: 18px;
}
.env-sub {
  color: var(--app-text-secondary);
  font-size: 12px;
  margin-top: 4px;
}
.dev-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.dev-name {
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 8px;
}
.dev-sub {
  color: var(--app-text-secondary);
  font-size: 12px;
  margin-top: 4px;
}
.pair-code {
  font-size: 26px;
  font-weight: 700;
  letter-spacing: 4px;
  text-align: center;
  padding: 12px;
  border: 1px dashed var(--app-border);
  border-radius: var(--app-radius);
}
.pair-tip {
  text-align: center;
  color: var(--app-text-secondary);
  font-size: 12px;
  margin: 8px 0 16px;
}
.qr-row {
  display: flex;
  gap: 16px;
  align-items: flex-start;
}
.qr-row img {
  width: 220px;
  height: 220px;
  border: 1px solid var(--app-border);
  border-radius: var(--app-radius);
  background: #fff;
}
.qr-side {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.pair-payload {
  font-size: 12px;
  word-break: break-all;
  color: var(--app-text-secondary);
  background: var(--app-fill-quaternary, rgba(0, 0, 0, 0.04));
  padding: 8px;
  border-radius: var(--app-radius);
}
</style>
