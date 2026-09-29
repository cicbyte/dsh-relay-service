import { request } from '@/utils/request'

export interface PairingView {
  code: string
  role: string
  name: string
  /** 绑定环境（room hex8；空=不绑） */
  room: string
  expiresAt: number
}

/** 签发一次性配对码（600s/单次/失败熔断）；room 非空即绑环境 */
export function issuePairing(role: string, name: string, room = '') {
  return request<PairingView>({
    url: '/api/pairing-codes',
    method: 'post',
    data: { role, name, room },
  })
}
