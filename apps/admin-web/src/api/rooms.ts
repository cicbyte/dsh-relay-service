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
  /** 归属用户（0=未归属） */
  ownerId: number
  /** 归属用户名 */
  ownerName: string
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

/** 删除环境（连带删设备与配对码，在线连接即时踢线） */
export function removeRoom(room: string) {
  return request<void>({ url: `/api/rooms/${room}`, method: 'delete' })
}
