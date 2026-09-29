<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { message, Modal } from 'ant-design-vue'
import { DeleteOutlined, PlusOutlined, ReloadOutlined, SafetyOutlined } from '@ant-design/icons-vue'
import { createUser, listUsers, removeUser, resetUserPassword, type AdminUser } from '@/api/users'
import { tokenStore } from '@/utils/request'
import { fmtDateTime } from '@/utils/format'

const loading = ref(false)
const rows = ref<AdminUser[]>([])

async function load() {
  loading.value = true
  try {
    rows.value = await listUsers()
  } finally {
    loading.value = false
  }
}
onMounted(load)

// ---- 新建用户 ----
const createOpen = ref(false)
const creating = ref(false)
const form = reactive({ username: '', password: '', role: 'user' as 'admin' | 'user' })

function openCreate() {
  form.username = ''
  form.password = ''
  form.role = 'user'
  createOpen.value = true
}

async function onCreate() {
  if (!form.username.trim()) {
    message.warning('请填写用户名')
    return
  }
  if (form.password.length < 8) {
    message.warning('密码至少 8 位')
    return
  }
  creating.value = true
  try {
    await createUser(form.username.trim(), form.password, form.role)
    message.success('用户已创建')
    createOpen.value = false
    await load()
  } finally {
    creating.value = false
  }
}

// ---- 重置密码 ----
const resetOpen = ref(false)
const resetTarget = ref<AdminUser | null>(null)
const resetPwd = ref('')
const resetting = ref(false)

function openReset(u: AdminUser) {
  resetTarget.value = u
  resetPwd.value = ''
  resetOpen.value = true
}

async function onReset() {
  if (!resetTarget.value) return
  if (resetPwd.value.length < 8) {
    message.warning('密码至少 8 位')
    return
  }
  resetting.value = true
  try {
    await resetUserPassword(resetTarget.value.id, resetPwd.value)
    message.success('已重置，该用户所有端需重新登录')
    resetOpen.value = false
    await load()
  } finally {
    resetting.value = false
  }
}

// ---- 删除 ----
function onDelete(u: AdminUser) {
  Modal.confirm({
    title: '删除用户',
    content: `删除 ${u.username}？将连带删除其全部环境、设备与未用配对码，在线连接立即断开，不可恢复。`,
    okType: 'danger',
    async onOk() {
      await removeUser(u.id)
      message.success('已删除')
      await load()
    },
  })
}

const selfId = computed(() => {
  // profile 接口给 userId；这里用 tokenStore 里缓存的用户名兜底判 self
  return (u: AdminUser) => u.username === tokenStore.username
})

const columns = [
  { title: 'ID', dataIndex: 'id', width: 80 },
  { title: '用户名', dataIndex: 'username' },
  { title: '角色', key: 'role', width: 110 },
  { title: '创建时间', key: 'createdAt', width: 180 },
  { title: '最后登录', key: 'lastLoginAt', width: 180 },
  { title: '操作', key: 'action', width: 180 },
]

function roleLabel(role: string) {
  return role === 'admin' ? '管理员' : '普通用户'
}
</script>

<template>
  <div>
    <a-card title="用户管理">
      <template #extra>
        <a-space>
          <a-button @click="load"><ReloadOutlined /> 刷新</a-button>
          <a-button type="primary" @click="openCreate"><PlusOutlined /> 新建用户</a-button>
        </a-space>
      </template>
      <div class="page-tip">
        无自助注册：账号由管理员创建。角色仅两种——<b>管理员</b>（全部环境 + 用户管理）与
        <b>普通用户</b>（仅自己的环境/设备/配对码/审计）。
      </div>
      <a-table
        :columns="columns"
        :data-source="rows"
        :loading="loading"
        row-key="id"
        :pagination="false"
        size="middle"
      >
        <template #bodyCell="{ column, record }: any">
          <template v-if="column.key === 'role'">
            <a-tag :color="record.role === 'admin' ? 'gold' : 'blue'">{{ roleLabel(record.role) }}</a-tag>
          </template>
          <template v-else-if="column.key === 'createdAt'">{{ fmtDateTime(record.createdAt) }}</template>
          <template v-else-if="column.key === 'lastLoginAt'">
            {{ record.lastLoginAt ? fmtDateTime(record.lastLoginAt) : '从未登录' }}
          </template>
          <template v-else-if="column.key === 'action'">
            <a-space>
              <a-button size="small" @click="openReset(record as AdminUser)">
                <SafetyOutlined /> 重置密码
              </a-button>
              <a-button
                size="small"
                danger
                :disabled="selfId(record as AdminUser)"
                :title="selfId(record as AdminUser) ? '不能删除自己' : ''"
                @click="onDelete(record as AdminUser)"
              >
                <DeleteOutlined /> 删除
              </a-button>
            </a-space>
          </template>
        </template>
      </a-table>
    </a-card>

    <a-modal v-model:open="createOpen" title="新建用户" :width="420" @ok="onCreate">
      <a-form layout="vertical">
        <a-form-item label="用户名" required>
          <a-input v-model:value="form.username" placeholder="如：zhangsan" />
        </a-form-item>
        <a-form-item label="初始密码" required>
          <a-input-password v-model:value="form.password" placeholder="至少 8 位" />
        </a-form-item>
        <a-form-item label="角色">
          <a-radio-group v-model:value="form.role">
            <a-radio value="user">普通用户（仅自己的环境）</a-radio>
            <a-radio value="admin">管理员（全部环境 + 用户管理）</a-radio>
          </a-radio-group>
        </a-form-item>
      </a-form>
      <template #footer>
        <a-button @click="createOpen = false">取消</a-button>
        <a-button type="primary" :loading="creating" @click="onCreate">创建</a-button>
      </template>
    </a-modal>

    <a-modal
      v-model:open="resetOpen"
      :title="`重置密码：${resetTarget?.username ?? ''}`"
      :width="420"
      :footer="null"
    >
      <a-form layout="vertical" @finish="onReset">
        <a-form-item label="新密码" required>
          <a-input-password v-model:value="resetPwd" placeholder="至少 8 位" />
        </a-form-item>
        <a-button type="primary" html-type="submit" :loading="resetting" block>重置</a-button>
      </a-form>
    </a-modal>
  </div>
</template>

<style scoped>
.page-tip {
  margin-bottom: 16px;
  font-size: 12px;
  color: var(--app-text-secondary);
}
</style>
