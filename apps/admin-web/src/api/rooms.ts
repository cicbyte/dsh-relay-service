import { request } from '@/utils/request'

export interface RoomDevice {
  id: string
  name: string
  role: string
  createdAt: number
  lastSeenAt: number
  revoked: boolean
  online: boolean
}

export interface RoomView {
  room: string
  displayName: string
  createdAt: number
  hostOnline: boolean
  clientsOnline: number
  host: RoomDevice | null
  clients: RoomDevice[]
}

/** 环境列表（host/client 分组 + 实时在线态） */
export function listRooms() {
  return request<RoomView[]>({ url: '/api/rooms', method: 'get' })
}

/** 新建环境 */
export function createRoom(displayName: string) {
  return request<RoomView>({ url: '/api/rooms', method: 'post', data: { displayName } })
}

/** 重命名环境 */
export function renameRoom(room: string, displayName: string) {
  return request<void>({ url: `/api/rooms/${room}`, method: 'put', data: { displayName } })
}
