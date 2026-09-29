import dayjs from 'dayjs'

/**
 * 时间格式化（全局统一）：epoch 秒/毫秒、ISO8601 → 本地时区 'YYYY-MM-DD HH:mm:ss'。
 * 无效值回退 '-'；解析失败原样返回（不吞异常数据）。
 */
export function fmtDateTime(v?: string | number | null): string {
  if (v === null || v === undefined || v === '' || v === 0) return '-'
  // 数字：epoch 秒（后端 ts/created_at 均为秒）或毫秒
  if (typeof v === 'number' || /^\d{10,13}$/.test(String(v))) {
    const n = Number(v)
    return dayjs(n < 1e12 ? n * 1000 : n).format('YYYY-MM-DD HH:mm:ss')
  }
  const d = dayjs(v)
  if (!d.isValid()) return String(v)
  return d.format('YYYY-MM-DD HH:mm:ss')
}

/** 列字段是否为时间字段（ProTable 自动格式化判定用） */
const TIME_FIELDS = new Set([
  'createdAt', 'updatedAt', 'lastSeenAt', 'expiresAt', 'ts',
  'lastLoginAt', 'startTime', 'endTime',
  'created_at', 'updated_at', 'last_seen_at', 'expires_at',
])

export function isTimeField(field?: string | number): boolean {
  if (field === undefined) return false
  const f = String(field)
  if (TIME_FIELDS.has(f)) return true
  return /(At|Time)$/.test(f) || /_at$|_time$/.test(f)
}
