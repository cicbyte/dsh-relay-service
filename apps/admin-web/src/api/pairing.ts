import { request } from '@/utils/request'

export interface PairingView {
  code: string
  role: string
  name: string
  expiresAt: number
}

/** 签发一次性配对码（600s/单次/失败熔断） */
export function issuePairing(role: string, name: string) {
  return request<PairingView>({
    url: '/api/pairing-codes',
    method: 'post',
    data: { role, name },
  })
}
