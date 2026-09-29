import { request } from '@/utils/request'

export interface WsStats {
  rooms: number
  hosts: number
  clients: number
}

export interface Overview {
  devicesTotal: number
  devicesOnline: number
  devicesRevoked: number
  pairingActive: number
  ws: WsStats
  authMode: string
  listenWs: string
  listenAdmin: string
}

export interface HealthDetail {
  status: string
  version: string
  db: string
  ws: WsStats
}

/** 运行概览 */
export function getOverview() {
  return request<Overview>({ url: '/api/status', method: 'get' })
}

/** 详细健康检查 */
export function getHealth() {
  return request<HealthDetail>({ url: '/api/health/detail', method: 'get' })
}
