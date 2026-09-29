<script setup lang="ts">
/**
 * ProTable：基于 a-table 的增强表格（列表页统一使用，对齐 byte-admin）。
 *
 * 内置能力：
 * - 分页/加载状态托管：传 fetch 即自动请求（返回 {list,total}）
 * - 列显示/隐藏：工具栏列设置面板，配置按 tableKey 持久化到 localStorage
 * - 密度切换（宽松/默认/紧凑）：同样持久化
 * - 手动刷新；工具栏左侧 slot 放业务按钮
 * - 其余 props/slots 全部透传 a-table（rowSelection、bodyCell 等）
 */
import { computed, onMounted, ref, useAttrs, watch } from 'vue'
import {
  ReloadOutlined, ColumnHeightOutlined, SettingOutlined,
} from '@ant-design/icons-vue'
import { fmtDateTime, isTimeField } from '@/utils/format'

export interface ProColumn {
  title: string
  dataIndex?: string
  key?: string
  width?: number | string
  ellipsis?: boolean | Record<string, unknown>
  fixed?: 'left' | 'right'
  /** 是否允许在列设置中隐藏（操作列等设为 false），默认 true */
  hideable?: boolean
  /** 透传 a-table customRender（时间列的默认格式化会被它覆盖） */
  customRender?: (opt: { text: unknown }) => unknown
}

interface FetchParams {
  pageNum: number
  pageSize: number
}

const props = withDefaults(
  defineProps<{
    /** 列配置持久化键（必填，全局唯一） */
    tableKey: string
    columns: ProColumn[]
    /** 请求函数：返回 {list,total}；与 data 二选一 */
    fetch?: (params: FetchParams) => Promise<{ list: unknown[]; total: number }>
    /** 静态数据（无分页场景） */
    data?: unknown[]
    /** 是否分页（默认 true；静态数据自动关闭） */
    pagination?: boolean
    /** 初始每页条数 */
    pageSize?: number
    /** 行主键字段（默认 id） */
    rowKey?: string
  }>(),
  { fetch: undefined, data: undefined, pagination: true, pageSize: 10, rowKey: 'id' },
)

const attrs = useAttrs()

// ---------- 数据加载 ----------
const list = ref<unknown[]>([])
const total = ref(0)
const pageNum = ref(1)
const pageSize = ref(props.pageSize)
const loading = ref(false)

const dataSource = computed(() => (props.data ? props.data : list.value))

async function load() {
  if (!props.fetch) return
  loading.value = true
  try {
    const r = await props.fetch({ pageNum: pageNum.value, pageSize: pageSize.value })
    list.value = r.list
    total.value = r.total
  } finally {
    loading.value = false
  }
}

/** 重置到第一页并请求（搜索按钮调用） */
function reload() {
  pageNum.value = 1
  load()
}

function refresh() {
  load()
}

defineExpose({ reload, refresh })

onMounted(load)

// ---------- 列显示/隐藏（持久化） ----------
const hiddenKey = computed(() => `protable:${props.tableKey}:hidden`)
const hiddenKeys = ref<string[]>([])

function readHidden(): string[] {
  try {
    return JSON.parse(localStorage.getItem(hiddenKey.value) ?? '[]')
  } catch {
    return []
  }
}
watch(hiddenKey, () => { hiddenKeys.value = readHidden() }, { immediate: true })

function colKey(c: ProColumn): string {
  return (c.key ?? c.dataIndex ?? c.title) as string
}

const settableColumns = computed(() => props.columns.filter((c) => c.hideable !== false))

const visibleColumns = computed(() =>
  props.columns
    .filter((c) => !hiddenKeys.value.includes(colKey(c)))
    // 时间列统一格式化（epoch 秒/毫秒 → 本地时间）；业务 customRender 优先
    .map((c) => {
      if (c.customRender) return c
      const field = (c.dataIndex ?? c.key) as string | undefined
      if (!isTimeField(field)) return c
      return { ...c, customRender: ({ text }: { text: unknown }) => fmtDateTime(text as number) }
    }),
)

function onHiddenChange(checkedKeys: (string | number | boolean)[]) {
  const checked = checkedKeys.map(String)
  hiddenKeys.value = settableColumns.value.map(colKey).filter((k) => !checked.includes(k))
  localStorage.setItem(hiddenKey.value, JSON.stringify(hiddenKeys.value))
}

function resetColumns() {
  hiddenKeys.value = []
  localStorage.setItem(hiddenKey.value, '[]')
}

// ---------- 密度（持久化） ----------
const densityKey = computed(() => `protable:${props.tableKey}:density`)
type Size = 'large' | 'middle' | 'small'
const size = ref<Size>('middle')
watch(densityKey, () => {
  size.value = (localStorage.getItem(densityKey.value) as Size) || 'middle'
}, { immediate: true })

const DENSITY_OPTIONS: { value: Size; label: string }[] = [
  { value: 'large', label: '宽松' },
  { value: 'middle', label: '默认' },
  { value: 'small', label: '紧凑' },
]

function onDensityChange(v: Size) {
  size.value = v
  localStorage.setItem(densityKey.value, v)
}

// ---------- 分页 ----------
const usePagination = computed(() => (props.data ? false : props.pagination))

const paginationProps = computed(() =>
  usePagination.value
    ? {
        current: pageNum.value,
        pageSize: pageSize.value,
        total: total.value,
        showSizeChanger: true,
        showTotal: (n: number) => `共 ${n} 条`,
      }
    : false,
)

function onTableChange(pag: { current?: number; pageSize?: number }) {
  pageNum.value = pag.current ?? 1
  pageSize.value = pag.pageSize ?? 10
  load()
}
</script>

<template>
  <div class="pro-table">
    <div class="pro-toolbar">
      <div class="pro-toolbar-left">
        <slot name="toolBar" />
      </div>
      <div class="pro-toolbar-right">
        <a-tooltip title="刷新">
          <span class="tool-btn" @click="refresh()">
            <ReloadOutlined />
          </span>
        </a-tooltip>
        <a-tooltip title="密度">
          <a-dropdown :trigger="['click']">
            <span class="tool-btn">
              <ColumnHeightOutlined />
            </span>
            <template #overlay>
              <a-menu :selected-keys="[size]" @click="({ key }: any) => onDensityChange(key)">
                <a-menu-item v-for="d in DENSITY_OPTIONS" :key="d.value">{{ d.label }}</a-menu-item>
              </a-menu>
            </template>
          </a-dropdown>
        </a-tooltip>
        <a-tooltip title="列设置">
          <a-popover trigger="click" placement="bottomRight">
            <span class="tool-btn">
              <SettingOutlined />
            </span>
            <template #content>
              <div class="col-setting">
                <div class="col-setting-header">
                  <a-checkbox
                    :checked="hiddenKeys.length === 0"
                    :indeterminate="hiddenKeys.length > 0 && hiddenKeys.length < settableColumns.length"
                    @change="(e: any) => (e.target.checked ? resetColumns() : onHiddenChange([]))"
                  >
                    列显示
                  </a-checkbox>
                  <a @click="resetColumns">重置</a>
                </div>
                <a-checkbox-group
                  :value="settableColumns.filter(c => !hiddenKeys.includes(colKey(c))).map(colKey)"
                  class="col-setting-body"
                  @change="onHiddenChange"
                >
                  <div v-for="c in settableColumns" :key="colKey(c)" class="col-setting-item">
                    <a-checkbox :value="colKey(c)">
                      {{ c.title }}
                    </a-checkbox>
                  </div>
                </a-checkbox-group>
              </div>
            </template>
          </a-popover>
        </a-tooltip>
        <slot name="toolBarRight" />
      </div>
    </div>

    <a-table
      v-bind="attrs"
      :columns="visibleColumns"
      :data-source="dataSource"
      :loading="loading"
      :pagination="paginationProps"
      :size="size"
      :row-key="rowKey"
      @change="(pag: any) => onTableChange(pag)"
    >
      <template v-for="(_, name) in $slots" :key="String(name)" #[name]="slotProps">
        <slot :name="name" v-bind="slotProps ?? {}" />
      </template>
    </a-table>
  </div>
</template>

<style scoped>
.pro-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
  gap: 12px;
}

.pro-toolbar-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.pro-toolbar-right {
  display: flex;
  align-items: center;
  gap: 4px;
}

.tool-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  color: var(--app-text-secondary);
  cursor: pointer;
}

.tool-btn:hover {
  background: var(--app-hover-bg);
  color: var(--app-primary);
}

.col-setting {
  width: 180px;
}

.col-setting-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid var(--app-border);
  padding-bottom: 8px;
  margin-bottom: 8px;
}

.col-setting-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 280px;
  overflow-y: auto;
}

.col-setting-item :deep(.ant-checkbox-wrapper) {
  width: 100%;
}
</style>
