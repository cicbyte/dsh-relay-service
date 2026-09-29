import { request } from '@/utils/request'

export interface AdminUser {
  id: number
  username: string
  /** admin | user */
  role: string
  createdAt: number
  lastLoginAt: number
}

/** 用户列表（仅 admin） */
export function listUsers() {
  return request<AdminUser[]>({ url: '/api/users', method: 'get' })
}

/** 新建用户（仅 admin；无自助注册） */
export function createUser(username: string, password: string, role: 'admin' | 'user') {
  return request<AdminUser>({
    url: '/api/users',
    method: 'post',
    data: { username, password, role },
  })
}

/** 重置用户密码（仅 admin；该用户所有端下线） */
export function resetUserPassword(id: number, password: string) {
  return request<void>({
    url: `/api/users/${id}/password`,
    method: 'put',
    data: { password },
  })
}

/** 删除用户（仅 admin；连带删其环境/设备/配对码） */
export function removeUser(id: number) {
  return request<void>({ url: `/api/users/${id}`, method: 'delete' })
}
