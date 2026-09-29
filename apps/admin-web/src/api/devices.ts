import { request } from '@/utils/request'

export interface DeviceView {
  id: string
  name: string
  role: string
  room: string
  createdAt: number
  lastSeenAt: number
  revoked: boolean
  online: boolean
}

/** 设备列表（含在线状态） */
export function listDevices() {
  return request<DeviceView[]>({ url: '/api/devices', method: 'get' })
}

/** 吊销：立即踢线，令牌作废 */
export function revokeDevice(id: string) {
  return request<void>({ url: `/api/devices/${id}/revoke`, method: 'post' })
}

/** 轮换令牌：新明文只返回一次 */
export function rotateDevice(id: string) {
  return request<{ token: string }>({ url: `/api/devices/${id}/rotate`, method: 'post' })
}

/** 删除设备 */
export function deleteDevice(id: string) {
  return request<void>({ url: `/api/devices/${id}`, method: 'delete' })
}
