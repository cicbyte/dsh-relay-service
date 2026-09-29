<template>
  <div class="env-page">
    <!-- 左：环境列表 -->
    <div class="env-list">
      <a-button type="primary" block @click="onCreateEnv">
        <PlusOutlined /> 新建环境
      </a-button>
      <div class="env-scroll">
        <a-dropdown v-for="r in rooms" :key="r.room" :trigger="['contextmenu']">
          <div
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
          <template #overlay>
            <a-menu @click="({ key }: any) => onCtxMenu(String(key), r)">
              <a-menu-item key="rename"><EditOutlined /> 重命名</a-menu-item>
              <a-menu-item key="pair-host"><KeyOutlined /> 生成主端配对码</a-menu-item>
              <a-menu-item key="pair-client"><MobileOutlined /> 生成手机配对码</a-menu-item>
              <a-menu-item key="copy-id"><CopyOutlined /> 复制环境 ID</a-menu-item>
              <a-menu-divider />
              <a-menu-item key="delete" danger><DeleteOutlined /> 删除环境</a-menu-item>
            </a-menu>
          </template>
        </a-dropdown>
        <div v-if="!rooms.length" class="list-empty">
          还没有环境<br />点上方「新建环境」开始
        </div>
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
            <a-button type="primary" @click="openPair('host')"><KeyOutlined /> 主端配对码</a-button>
            <a-button @click="openPair('client')"><MobileOutlined /> 手机配对码</a-button>
          </a-space>
        </div>
      </a-card>

      <!-- 有设备（主端或手机任一存在）：常规两卡；全空：分步接入引导 -->
      <template v-if="current.host || current.clients.length">
      <a-card title="主端（Host）" class="sec-card">
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
        <div v-else class="dev-empty">
          <DisconnectOutlined class="dev-empty-icon" />
          <div class="dev-empty-title">主端尚未接入</div>
          <div class="dev-empty-desc">生成主端配对码，在 dsh「手机通道」设置里填入或扫码（桌面端、网页端均可担任主端）；上线后此处亮起绿点</div>
          <a-button type="primary" @click="openPair('host')"><KeyOutlined /> 生成主端配对码</a-button>
        </div>
      </a-card>

      <a-card title="手机 / 平板（Clients）" class="sec-card">
        <a-table
          v-if="current.clients.length"
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
        <div v-if="!current.clients.length" class="dev-empty">
          <MobileOutlined class="dev-empty-icon" />
          <div class="dev-empty-title">还没有手机接入</div>
          <div class="dev-empty-desc">生成手机配对码，家人用 dsh 手机 App 扫码即连（也可在主端的手机通道设置里生成）</div>
          <a-button @click="openPair('client')"><MobileOutlined /> 生成手机配对码</a-button>
        </div>
      </a-card>
      </template>

      <!-- 全空：分步接入引导（先主端后手机） -->
      <a-card v-else class="sec-card">
        <template #title>快速接入</template>
        <div class="guide">
          <div class="guide-step">
            <div class="guide-no on">1</div>
            <div class="guide-body">
              <div class="guide-title">先接入主端（Host）</div>
              <div class="guide-desc">生成主端配对码，在 dsh 的「手机通道」设置里填入或扫码——桌面端、网页端均可担任主端。上线后此处亮起绿点。</div>
              <a-button type="primary" @click="openPair('host')"><KeyOutlined /> 生成主端配对码</a-button>
            </div>
          </div>
          <div class="guide-step">
            <div class="guide-no">2</div>
            <div class="guide-body">
              <div class="guide-title">再接入手机（可多台）</div>
              <div class="guide-desc">生成手机配对码，用 dsh 手机 App 扫码加入本环境；主端上线后即可互通。</div>
              <a-button @click="openPair('client')"><MobileOutlined /> 生成手机配对码</a-button>
            </div>
          </div>
        </div>
      </a-card>
    </div>
    <div v-else class="env-hero">
      <CloudServerOutlined class="hero-icon" />
      <h2 class="hero-title">{{ rooms.length ? '选择一个环境' : '还没有环境' }}</h2>
      <p class="hero-desc">
        {{ rooms.length
          ? '从左侧列表选择环境，查看接入状态与配对码。'
          : '环境 = 一个 dsh 端 + 多台手机。创建后生成配对码，主端与手机扫码即可接入。' }}
      </p>
      <a-button v-if="!rooms.length" type="primary" size="large" @click="onCreateEnv">
        <PlusOutlined /> 新建环境
      </a-button>
    </div>

    <!-- 出码 + 二维码 -->
    <a-modal v-model:open="pairOpen" :title="pairRole === 'host' ? '主端配对码' : '手机配对码'" :footer="null" :width="560">
      <a-form layout="vertical" :model="pairForm" @finish="onIssue">
        <a-form-item label="中继地址（手机可达的 relay 地址）" name="addr">
          <a-input v-model:value="pairForm.addr" placeholder="1.2.3.4:8787" />
        </a-form-item>
        <a-form-item label="设备名" name="name">
          <a-input v-model:value="pairForm.name" :placeholder="pairRole === 'host' ? '如：家里 dsh' : '如：小米14'" />
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
    <a-modal v-model:open="nameModal.open" :title="nameModal.title" @ok="onNameOk" :width="420">
      <a-input
        v-model:value="nameModal.value"
        placeholder="环境名（如：家里、公司）——同一环境可接入一个主端与多台手机"
        @press-enter="onNameOk"
      />
    </a-modal>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { message, Modal } from 'ant-design-vue'
import {
  CloudServerOutlined, CopyOutlined, DeleteOutlined, DesktopOutlined, DisconnectOutlined,
  EditOutlined, KeyOutlined, MobileOutlined, PlusOutlined,
} from '@ant-design/icons-vue'
import QRCode from 'qrcode'
import { fmtDateTime } from '@/utils/format'
import type { RoomDevice, RoomView } from '@/api/rooms'
import { createRoom, listRooms, removeRoom, renameRoom } from '@/api/rooms'
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

/** 环境卡片右键菜单 */
function onCtxMenu(key: string, r: RoomView) {
  selectedRoom.value = r.room // 菜单动作作用于右键的环境
  if (key === 'rename') onRename()
  else if (key === 'pair-host') openPair('host')
  else if (key === 'pair-client') openPair('client')
  else if (key === 'copy-id') copyText(r.room)
  else if (key === 'delete') onDeleteEnv(r)
}

function onDeleteEnv(r: RoomView) {
  Modal.confirm({
    title: `删除环境「${r.displayName}」？`,
    content: '将连带删除该环境的全部设备与未用配对码，在线连接立即断开，操作不可恢复。',
    okText: '删除',
    okType: 'danger',
    cancelText: '取消',
    async onOk() {
      await removeRoom(r.room)
      message.success('环境已删除')
      if (selectedRoom.value === r.room) selectedRoom.value = ''
      await load()
    },
  })
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
/* 空态：左列表紧凑提示 */
.list-empty {
  margin-top: 12px;
  padding: 16px 12px;
  text-align: center;
  font-size: 12px;
  line-height: 2;
  color: var(--app-text-muted);
  border: 1px dashed var(--app-border);
  border-radius: var(--app-radius);
}
/* 空态：分步接入引导 */
.guide {
  display: flex;
  flex-direction: column;
  gap: 20px;
  padding: 8px 4px;
}
.guide-step {
  display: flex;
  gap: 14px;
  align-items: flex-start;
}
.guide-no {
  flex: none;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 600;
  color: var(--app-text-muted);
  background: var(--app-fill-quaternary, rgba(0, 0, 0, 0.06));
}
.guide-no.on {
  color: #fff;
  background: var(--app-primary);
}
.guide-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: flex-start;
}
.guide-title {
  font-size: 15px;
  font-weight: 600;
}
.guide-desc {
  max-width: 520px;
  font-size: 12px;
  line-height: 1.8;
  color: var(--app-text-secondary);
  margin-bottom: 6px;
}
/* 空态：卡片内引导式（图标 + 标题 + 说明 + 主操作） */
.dev-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 32px 16px;
  text-align: center;
}
.dev-empty-icon {
  font-size: 36px;
  color: var(--app-text-muted);
}
.dev-empty-title {
  font-size: 15px;
  font-weight: 600;
}
.dev-empty-desc {
  max-width: 380px;
  font-size: 12px;
  line-height: 1.8;
  color: var(--app-text-secondary);
  margin-bottom: 8px;
}
/* 空态：右侧首屏 hero（无环境/未选择） */
.env-hero {
  flex: 1;
  min-height: 420px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  text-align: center;
  background: var(--app-card-bg);
  border: 1px dashed var(--app-border);
  border-radius: var(--app-radius);
  padding: 40px 24px;
}
.hero-icon {
  font-size: 56px;
  color: color-mix(in srgb, var(--app-primary) 55%, var(--app-text-muted));
}
.hero-title {
  margin: 0;
  font-size: 20px;
}
.hero-desc {
  margin: 0 0 12px;
  max-width: 420px;
  color: var(--app-text-secondary);
  line-height: 1.8;
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
