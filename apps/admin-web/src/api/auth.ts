import { request, tokenStore } from '@/utils/request'

export interface TokenPair {
  accessToken: string
  refreshToken: string
  username: string
}

export interface ProfileResult {
  userId: number
  username: string
}

/** 登录 */
export function login(username: string, password: string) {
  return request<TokenPair>({
    url: '/api/auth/login',
    method: 'post',
    data: { username, password },
    skipErrorToast: true,
  })
}

/** 刷新令牌（nonce 轮转，旧 refresh 立即失效） */
export function refresh(refreshToken: string) {
  return request<TokenPair>({
    url: '/api/auth/refresh',
    method: 'post',
    data: { refreshToken },
  })
}

/** 登出（refresh 失效） */
export function logout() {
  return request<void>({ url: '/api/auth/logout', method: 'post' })
}

/** 当前登录信息 */
export function getProfile() {
  return request<ProfileResult>({ url: '/api/auth/profile', method: 'get' })
}

/** 修改密码（其他端全部下线） */
export function changePassword(oldPassword: string, newPassword: string) {
  return request<void>({
    url: '/api/auth/password',
    method: 'post',
    data: { oldPassword, newPassword },
  })
}

export async function doLogout() {
  try {
    await logout()
  } catch {
    // 忽略：本地态照样清理
  }
  tokenStore.clear()
}
