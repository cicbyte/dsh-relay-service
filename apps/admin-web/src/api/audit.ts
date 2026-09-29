import { request } from '@/utils/request'

export interface AuditView {
  id: number
  ts: number
  event: string
  ip: string
  device?: string | null
  detail: string
}

/** 审计日志（时间倒序） */
export function listAudit(limit = 50) {
  return request<AuditView[]>({ url: '/api/audit', method: 'get', params: { limit } })
}
